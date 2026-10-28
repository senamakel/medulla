/* Copyright 2021 Aristocratos (jakob@qvantnet.com)

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

	   http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.

indent = tab
tab-size = 4
*/

#include "btop.hpp"

#include <algorithm>
#include <atomic>
#include <csignal>
#include <clocale>
#include <filesystem>
#include <iterator>
#include <mutex>
#include <optional>
#include <pthread.h>
#include <span>
#include <string_view>
#ifdef __FreeBSD__
	#include <pthread_np.h>
#endif
#include <thread>
#include <numeric>
#include <ranges>
#include <unistd.h>
#include <cmath>
#include <iostream>
#include <exception>
#include <tuple>
#include <regex>
#include <chrono>
#include <utility>
#include <semaphore>

#ifdef __APPLE__
	#include <CoreFoundation/CoreFoundation.h>
	#include <mach-o/dyld.h>
	#include <limits.h>
#endif

#ifdef __NetBSD__
	#include <sys/param.h>
	#include <sys/sysctl.h>
	#include <unistd.h>
#endif

#include <fmt/core.h>
#include <fmt/ostream.h>

#include "btop_cli.hpp"
#include "btop_config.hpp"
#include "btop_draw.hpp"
#include "btop_input.hpp"
#include "btop_log.hpp"
#include "btop_menu.hpp"
#include "btop_shared.hpp"
#include "btop_theme.hpp"
#include "btop_tools.hpp"
#include "btop_embed.hpp"

using std::atomic;
using std::cout;
using std::flush;
using std::min;
using std::string;
using std::string_view;
using std::to_string;
using std::vector;

namespace fs = std::filesystem;

using namespace Tools;
using namespace std::chrono_literals;
using namespace std::literals;

namespace Global {
	const vector<array<string, 2>> Banner_src = {
		{"#E62525", "██████╗ ████████╗ ██████╗ ██████╗"},
		{"#CD2121", "██╔══██╗╚══██╔══╝██╔═══██╗██╔══██╗   ██╗    ██╗"},
		{"#B31D1D", "██████╔╝   ██║   ██║   ██║██████╔╝ ██████╗██████╗"},
		{"#9A1919", "██╔══██╗   ██║   ██║   ██║██╔═══╝  ╚═██╔═╝╚═██╔═╝"},
		{"#801414", "██████╔╝   ██║   ╚██████╔╝██║        ╚═╝    ╚═╝"},
		{"#000000", "╚═════╝    ╚═╝    ╚═════╝ ╚═╝"},
	};
	const string Version = "1.4.8";

	int coreCount;
	string overlay;
	string clock;

	string fg_white = "\x1b[1;97m";
	string fg_green = "\x1b[1;92m";
	string fg_red = "\x1b[1;91m";

	uid_t real_uid, set_uid;

	fs::path self_path;

	string exit_error_msg;
	atomic<bool> thread_exception (false);

	bool debug{};

	uint64_t start_time;

	atomic<bool> resized (false);
	atomic<bool> quitting (false);
	atomic<bool> should_quit (false);
	atomic<bool> should_sleep (false);
	atomic<bool> _runner_started (false);
	atomic<bool> init_conf (false);
	atomic<bool> reload_conf (false);
}

namespace Runner {
	static pthread_t runner_id;
} // namespace Runner

//* Handler for SIGWINCH and general resizing events, does nothing if terminal hasn't been resized unless force=true
void term_resize(bool force) {
	static atomic<bool> resizing (false);
	if (Input::polling) {
		Global::resized = true;
		Input::interrupt();
		return;
	}
	atomic_lock lck(resizing, true);
	if (auto refreshed = Term::refresh(true); refreshed or force) {
		if (force and refreshed) force = false;
	}
	else return;
#ifdef GPU_SUPPORT
	static const array<string, 10> all_boxes = {"gpu5", "cpu", "mem", "net", "proc", "gpu0", "gpu1", "gpu2", "gpu3", "gpu4"};
#else
	static const array<string, 5> all_boxes = {"", "cpu", "mem", "net", "proc"};
#endif
	Global::resized = true;
	if (Runner::active) Runner::stop();
	Term::refresh();
	Config::unlock();

	auto boxes = Config::getS("shown_boxes");
	auto min_size = Term::get_min_size(boxes);
	auto minWidth = min_size.at(0), minHeight = min_size.at(1);

	while (not force or (Term::width < minWidth or Term::height < minHeight)) {
		sleep_ms(100);
		if (Term::width < minWidth or Term::height < minHeight) {
			int width = Term::width, height = Term::height;
			cout << fmt::format("{clear}{fg_white}"
					"{mv1}Terminal size too small:"
					"{mv2} Width = {fg_width}{width} {fg_white}Height = {fg_height}{height}"
					"{mv3}{fg_white}Needed for current config:"
					"{mv4}Width = {minWidth} Height = {minHeight}",
					"clear"_a = Term::clear, "fg_white"_a = Global::fg_white,
					"mv1"_a = Mv::to((height / 2) - 2, (width / 2) - 11),
					"mv2"_a = Mv::to((height / 2) - 1, (width / 2) - 10),
						"fg_width"_a = (width < minWidth ? Global::fg_red : Global::fg_green),
						"width"_a = width,
						"fg_height"_a = (height < minHeight ? Global::fg_red : Global::fg_green),
						"height"_a = height,
					"mv3"_a = Mv::to((height / 2) + 1, (width / 2) - 12),
					"mv4"_a = Mv::to((height / 2) + 2, (width / 2) - 10),
						"minWidth"_a = minWidth,
						"minHeight"_a = minHeight
			) << std::flush;

			bool got_key = false;
			for (; not Term::refresh() and not got_key; got_key = Input::poll(10));
			if (got_key) {
				auto key = Input::get();
				if (key == "q")
					clean_quit(0);
				else if (key.size() == 1 and isint(key)) {
					auto intKey = stoi(key);
				#ifdef GPU_SUPPORT
					if ((intKey == 0 and Gpu::count >= 5) or (intKey >= 5 and intKey - 4 <= Gpu::count)) {
				#else
					if (intKey > 0 and intKey < 5) {
				#endif
						const auto& box = all_boxes.at(intKey);
						Config::current_preset.reset();
						Config::toggle_box(box);
						boxes = Config::getS("shown_boxes");
					}
				}
			}
			min_size = Term::get_min_size(boxes);
			minWidth = min_size.at(0);
			minHeight = min_size.at(1);
		}
		else if (not Term::refresh()) break;
	}

	Input::interrupt();
}

//* Fatal-error handler. Upstream stops threads, restores the terminal and ends
//* the process; embedded in Medulla there is no process to end, so it unwinds
//* btop's own thread instead and the host shows the message. An operator's
//* request to quit goes through Embed::request_leave(), never through here.
void clean_quit(int sig) {
	if (Global::quitting) return;
	Global::quitting = true;
	string message = Global::exit_error_msg;
	if (message.empty()) message = fmt::format("btop stopped (code {})", sig);
	Logger::error("{}", message);
	throw Embed::Fatal{message};
}

//* Config init
void init_config(bool low_color, std::optional<std::string>& filter) {
	atomic_lock lck(Global::init_conf);
	vector<string> load_warnings;
	Config::load(Config::conf_file, load_warnings);
	Config::set("lowcolor", (low_color ? true : not Config::getB("truecolor")));

	static bool first_init = true;

	if (Global::debug and first_init) {
		Logger::set_log_level(Logger::Level::DEBUG);
		Logger::debug("Running in DEBUG mode!");
	}
	else Logger::set_log_level(Config::getS("log_level"));

	if (filter.has_value()) {
		Config::set("proc_filter", filter.value());
	}

	static string log_level;
	if (const string current_level = Config::getS("log_level"); log_level != current_level) {
		log_level = current_level;
		Logger::info("Logger set to {}", (Global::debug ? "DEBUG" : log_level));
	}

	for (const auto& err_str : load_warnings) Logger::warning("{}", err_str);
	first_init = false;
}

//* Manages secondary thread for collection and drawing of boxes
namespace Runner {
	atomic_waiting_lock active;
	atomic<bool> stopping (false);
	atomic<bool> waiting (false);
	atomic<bool> redraw (false);
	atomic<bool> coreNum_reset (false);

	static inline auto set_active(bool value) noexcept {
		active.store(value);
	}

	//* Setup semaphore for triggering thread to do work
	// TODO: This can be made a local without too much effort.
	std::binary_semaphore do_work { 0 };
	inline void thread_wait() { do_work.acquire(); }
	inline void thread_trigger() { do_work.release(); }

	//* Wrapper for raising privileges when using SUID bit
	class gain_priv {
		int status = -1;
	public:
		gain_priv() {
			if (Global::real_uid != Global::set_uid)
				this->status = seteuid(Global::set_uid);
		}
		~gain_priv() noexcept {
			if (status == 0)
				status = seteuid(Global::real_uid);
		}
		gain_priv(const gain_priv& other) = delete;
		gain_priv& operator=(const gain_priv& other) = delete;
		gain_priv(gain_priv&& other) = delete;
		gain_priv& operator=(gain_priv&& other) = delete;
	};

	string output;
	string empty_bg;
	bool pause_output{};
	sigset_t mask;
	std::mutex mtx;

	enum debug_actions {
		collect_begin,
		collect_done,
		draw_begin,
		draw_begin_only,
		draw_done
	};

	enum debug_array {
		collect,
		draw
	};

	string debug_bg;
	std::unordered_map<string, array<uint64_t, 2>> debug_times;

	class MyNumPunct : public std::numpunct<char>
	{
	protected:
		virtual char do_thousands_sep() const override { return '\''; }
		virtual std::string do_grouping() const override { return "\03"; }
	};


	struct runner_conf {
		vector<string> boxes;
		bool no_update;
		bool force_redraw;
		bool background_update;
		string overlay;
		string clock;
	};

	struct runner_conf current_conf;

	static void debug_timer(const char* name, const int action) {
		switch (action) {
			case collect_begin:
				debug_times[name].at(collect) = time_micros();
				return;
			case collect_done:
				debug_times[name].at(collect) = time_micros() - debug_times[name].at(collect);
				debug_times["total"].at(collect) += debug_times[name].at(collect);
				return;
			case draw_begin_only:
				debug_times[name].at(draw) = time_micros();
				return;
			case draw_begin:
				debug_times[name].at(draw) = time_micros();
				debug_times[name].at(collect) = debug_times[name].at(draw) - debug_times[name].at(collect);
				debug_times["total"].at(collect) += debug_times[name].at(collect);
				return;
			case draw_done:
				debug_times[name].at(draw) = time_micros() - debug_times[name].at(draw);
				debug_times["total"].at(draw) += debug_times[name].at(draw);
				return;
		}
	}

	//? ------------------------------- Secondary thread: async launcher and drawing ----------------------------------
	static void * _runner(void *) {
		//? Block some signals in this thread to avoid deadlock from any signal handlers trying to stop this thread
		sigemptyset(&mask);
		// sigaddset(&mask, SIGINT);
		// sigaddset(&mask, SIGTSTP);
		sigaddset(&mask, SIGWINCH);
		sigaddset(&mask, SIGTERM);
		pthread_sigmask(SIG_BLOCK, &mask, nullptr);

		// TODO: On first glance it looks redudant with `Runner::active`.
		std::lock_guard lock {mtx};

		//* ----------------------------------------------- THREAD LOOP -----------------------------------------------
		while (not Global::quitting) {
			thread_wait();
			atomic_wait_for(active, true, 30'000);
			if (active) {
				Global::exit_error_msg = "Runner thread failed to get active lock (30s)!";
				Global::thread_exception = true;
				Input::interrupt();
				stopping = true;
			}
			if (stopping or Global::resized) {
				sleep_ms(1);
				continue;
			}

			//? Atomic lock used for blocking non thread-safe actions in main thread
			auto lck = active.lock();

			//? Set effective user if SUID bit is set
			gain_priv powers{};

			auto& conf = current_conf;

			//! DEBUG stats
			if (Global::debug) {
                if (debug_bg.empty() or redraw)
                    Runner::debug_bg = Draw::createBox(2, 2, 33,
					#ifdef GPU_SUPPORT
						9,
					#else
						8,
					#endif
					"", true, "μs");

				debug_times.clear();
				debug_times["total"] = {0, 0};
			}

			output.clear();

			//* Run collection and draw functions for all boxes
			try {
#if defined(GPU_SUPPORT)
				//? GPU data collection
				const bool gpu_in_cpu_panel = Gpu::gpu_names.size() > 0 and (
					Config::getS("cpu_graph_lower").starts_with("gpu-")
					or (Config::getS("cpu_graph_lower") == "Auto")
					or Config::getS("cpu_graph_upper").starts_with("gpu-")
					or (Gpu::shown == 0 and Config::getS("show_gpu_info") != "Off")
				);

				vector<unsigned int> gpu_panels = {};
				for (auto& box : conf.boxes)
					if (box.starts_with("gpu"))
						gpu_panels.push_back(box.back()-'0');

				vector<Gpu::gpu_info> gpus;
				if (gpu_in_cpu_panel or not gpu_panels.empty()) {
					if (Global::debug) debug_timer("gpu", collect_begin);
					gpus = Gpu::collect(conf.no_update);
					if (Global::debug) debug_timer("gpu", collect_done);
				}
				auto& gpus_ref = gpus;
#endif // GPU_SUPPORT

				//? CPU
				if (v_contains(conf.boxes, "cpu")) {
					try {
						if (Global::debug) debug_timer("cpu", collect_begin);

						//? Start collect
						auto cpu = Cpu::collect(conf.no_update);

						if (coreNum_reset) {
							coreNum_reset = false;
							Cpu::core_mapping = Cpu::get_core_mapping();
							Global::resized = true;
							Input::interrupt();
							continue;
						}

						if (Global::debug) debug_timer("cpu", draw_begin);

						//? Draw box
						if (not pause_output) {
							output += Cpu::draw(
								cpu,
#if defined(GPU_SUPPORT)
								gpus_ref,
#endif // GPU_SUPPORT
								conf.force_redraw,
								conf.no_update
							);
						}

						if (Global::debug) debug_timer("cpu", draw_done);
					}
					catch (const std::exception& e) {
						throw std::runtime_error("Cpu:: -> " + string{e.what()});
					}
				}
			#ifdef GPU_SUPPORT
				//? GPU
				if (not gpu_panels.empty() and not gpus_ref.empty()) {
					try {
						if (Global::debug) debug_timer("gpu", draw_begin_only);

						//? Draw box
						if (not pause_output)
							for (unsigned long i = 0; i < gpu_panels.size(); ++i)
								output += Gpu::draw(gpus_ref[gpu_panels[i]], i, conf.force_redraw, conf.no_update);

						if (Global::debug) debug_timer("gpu", draw_done);
					}
					catch (const std::exception& e) {
                        throw std::runtime_error("Gpu:: -> " + string{e.what()});
					}
				}
			#endif
				//? MEM
				if (v_contains(conf.boxes, "mem")) {
					try {
						if (Global::debug) debug_timer("mem", collect_begin);

						//? Start collect
						auto mem = Mem::collect(conf.no_update);

						if (Global::debug) debug_timer("mem", draw_begin);

						//? Draw box
						if (not pause_output) output += Mem::draw(mem, conf.force_redraw, conf.no_update);

						if (Global::debug) debug_timer("mem", draw_done);
					}
					catch (const std::exception& e) {
						throw std::runtime_error("Mem:: -> " + string{e.what()});
					}
				}

				//? NET
				if (v_contains(conf.boxes, "net")) {
					try {
						if (Global::debug) debug_timer("net", collect_begin);

						//? Start collect
						auto net = Net::collect(conf.no_update);

						if (Global::debug) debug_timer("net", draw_begin);

						//? Draw box
						if (not pause_output) output += Net::draw(net, conf.force_redraw, conf.no_update);

						if (Global::debug) debug_timer("net", draw_done);
					}
					catch (const std::exception& e) {
						throw std::runtime_error("Net:: -> " + string{e.what()});
					}
				}

				//? PROC
				if (v_contains(conf.boxes, "proc")) {
					try {
						if (Global::debug) debug_timer("proc", collect_begin);

						//? Start collect
						auto proc = Proc::collect(conf.no_update);

						if (Global::debug) debug_timer("proc", draw_begin);

						//? Draw box
						if (not pause_output) output += Proc::draw(proc, conf.force_redraw, conf.no_update);

						if (Global::debug) debug_timer("proc", draw_done);
					}
					catch (const std::exception& e) {
						throw std::runtime_error("Proc:: -> " + string{e.what()});
					}
				}

			}
			catch (const std::exception& e) {
				Global::exit_error_msg = fmt::format("Exception in runner thread -> {}", e.what());
				Global::thread_exception = true;
				Input::interrupt();
				stopping = true;
			}

			if (stopping) {
				continue;
			}

			if (redraw or conf.force_redraw) {
				empty_bg.clear();
				redraw = false;
			}

			if (not pause_output) output += conf.clock;
			if (not conf.overlay.empty() and not conf.background_update) pause_output = true;
			if (output.empty() and not pause_output) {
				if (empty_bg.empty()) {
					const int x = Term::width / 2 - 10, y = Term::height / 2 - 10;
					output += Term::clear;
					empty_bg = fmt::format(
						"{banner}"
						"{mv1}{titleFg}{b}No boxes shown!"
						"{mv2}{hiFg}1 {mainFg}| Show CPU box"
						"{mv3}{hiFg}2 {mainFg}| Show MEM box"
						"{mv4}{hiFg}3 {mainFg}| Show NET box"
						"{mv5}{hiFg}4 {mainFg}| Show PROC box"
						"{mv6}{hiFg}5-0 {mainFg}| Show GPU boxes"
						"{mv7}{hiFg}esc {mainFg}| Show menu"
						"{mv8}{hiFg}q {mainFg}| Quit",
						"banner"_a = Draw::banner_gen(y, 0, true),
						"titleFg"_a = Theme::c("title"), "b"_a = Fx::b, "hiFg"_a = Theme::c("hi_fg"), "mainFg"_a = Theme::c("main_fg"),
						"mv1"_a = Mv::to(y+6, x),
						"mv2"_a = Mv::to(y+8, x),
						"mv3"_a = Mv::to(y+9, x),
						"mv4"_a = Mv::to(y+10, x),
						"mv5"_a = Mv::to(y+11, x),
						"mv6"_a = Mv::to(y+12, x-2),
						"mv7"_a = Mv::to(y+13, x-2),
						"mv8"_a = Mv::to(y+14, x)
					);
				}
				output += empty_bg;
			}

			//! DEBUG stats -->
			if (Global::debug and not Menu::active) {
				output += fmt::format("{pre}{box:5.5} {collect:>12.12} {draw:>12.12}{post}",
					"pre"_a = debug_bg + Theme::c("title") + Fx::b,
					"box"_a = "box", "collect"_a = "collect", "draw"_a = "draw",
					"post"_a = Theme::c("main_fg") + Fx::ub
				);
				static auto loc = std::locale(std::locale::classic(), new MyNumPunct);
			#ifdef GPU_SUPPORT
				for (const string name : {"cpu", "mem", "net", "proc", "gpu", "total"}) {
			#else
				for (const string name : {"cpu", "mem", "net", "proc", "total"}) {
			#endif
					if (not debug_times.contains(name)) debug_times[name] = {0,0};
					const auto& [time_collect, time_draw] = debug_times.at(name);
					if (name == "total") output += Fx::b;
					output += fmt::format(loc, "{mvLD}{name:5.5} {collect:12L} {draw:12L}",
						"mvLD"_a = Mv::l(31) + Mv::d(1),
						"name"_a = name,
						"collect"_a = time_collect,
						"draw"_a = time_draw
					);
				}
			}

			//? If overlay isn't empty, print output without color and then print overlay on top
			const bool term_sync = Config::getB("terminal_sync");
			cout << (term_sync ? Term::sync_start : "") << (conf.overlay.empty()
					? output
					: (output.empty() ? "" : Fx::ub + Theme::c("inactive_fg") + Fx::uncolor(output)) + conf.overlay)
				<< (term_sync ? Term::sync_end : "") << flush;
		}
		//* ----------------------------------------------- THREAD LOOP -----------------------------------------------
		return {};
	}
	//? ------------------------------------------ Secondary thread end -----------------------------------------------

	//* Runs collect and draw in a secondary thread, unlocks and locks config to update cached values
	void run(const string& box, bool no_update, bool force_redraw) {
		atomic_wait_for(active, true, 10'000);
		if (active) {
			Logger::warning("Runner thread slow (>10s), waiting up to 30s...");
			atomic_wait_for(active, true, 20'000);
		}
		if (active) {
			Global::exit_error_msg = "Runner thread stalled for 30s, exiting.";
			clean_quit(1);
		}
		if (stopping or Global::resized) return;

		if (box == "overlay") {
			const bool term_sync = Config::getB("terminal_sync");
			cout << (term_sync ? Term::sync_start : "") << Global::overlay << (term_sync ? Term::sync_end : "") << flush;
		}
		else if (box == "clock") {
			const bool term_sync = Config::getB("terminal_sync");
			cout << (term_sync ? Term::sync_start : "") << Global::clock << (term_sync ? Term::sync_end : "") << flush;
		}
		else {
			Config::unlock();
			Config::lock();

			current_conf = {
				(box == "all" ? Config::current_boxes : vector{box}),
				no_update, force_redraw,
				(not Config::getB("tty_mode") and Config::getB("background_update")),
				Global::overlay,
				Global::clock
			};

			if (Menu::active and not current_conf.background_update) Global::overlay.clear();

			thread_trigger();
			atomic_wait_for(active, false, 10);
		}


	}

	//* Stops any work being done in runner thread and checks for thread errors
	void stop() {
		stopping = true;
		auto lock = std::unique_lock {mtx, std::defer_lock};
		const auto is_runner_busy = !lock.try_lock();
		if (!is_runner_busy and not Global::quitting) {
			if (active) {
				set_active(false);
			}
			Global::exit_error_msg = "Runner thread died unexpectedly!";
			clean_quit(1);
		} else if (is_runner_busy) {
			atomic_wait_for(active, true, 30'000);
			if (active) {
				set_active(false);
				if (Global::quitting) {
					return;
				}
				else {
					Global::exit_error_msg = "No response from Runner thread (30s), quitting!";
					clean_quit(1);
				}
			}
			thread_trigger();
			atomic_wait_for(active, false, 100);
			atomic_wait_for(active, true, 100);
		}
		stopping = false;
	}

}

