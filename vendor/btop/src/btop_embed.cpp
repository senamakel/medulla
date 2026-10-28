// SPDX-License-Identifier: Apache-2.0
//
// State behind the Embed seam (btop_embed.hpp) and the C ABI the host links
// against (include/medulla_btop.h). btop's own loop lives in btop.cpp as
// btop_embed_main(); this file only owns the shared state it talks through.

#include "btop_embed.hpp"

#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstring>
#include <deque>
#include <mutex>
#include <thread>

#include "medulla_btop.h"

//* btop's thread body, defined in btop.cpp. Returns when Embed::stopping().
auto btop_embed_main() -> void;

namespace {
	std::mutex out_mtx;
	std::string out_buf;
	medulla_btop_waker waker = nullptr;
	void* waker_ctx = nullptr;

	std::mutex in_mtx;
	std::condition_variable in_cv;
	std::deque<std::string> in_queue;
	bool in_interrupted = false;
	bool shown = true;

	std::atomic<int> view_width{0};
	std::atomic<int> view_height{0};
	std::atomic<bool> leave_requested{false};
	std::atomic<bool> stop_requested{false};
	std::atomic<bool> started{false};
	std::atomic<bool> finished{false};

	std::mutex err_mtx;
	std::string fatal_message;

	std::mutex done_mtx;
	std::condition_variable done_cv;

	std::filesystem::path conf_dir;

	std::mutex agents_mtx;
	std::unordered_map<size_t, Embed::Agent> agent_table;
	uint64_t agent_generation = 0;

	void wake_host() {
		if (waker != nullptr) waker(waker_ctx);
	}

	void thread_entry() {
		try {
			btop_embed_main();
		}
		catch (const Embed::Fatal& fatal) {
			std::lock_guard lock{err_mtx};
			fatal_message = fatal.message.empty() ? "btop stopped unexpectedly" : fatal.message;
		}
		catch (const std::exception& e) {
			std::lock_guard lock{err_mtx};
			fatal_message = e.what();
		}
		catch (...) {
			std::lock_guard lock{err_mtx};
			fatal_message = "btop stopped on an unknown exception";
		}
		{
			std::lock_guard lock{done_mtx};
			finished = true;
		}
		done_cv.notify_all();
		wake_host();
	}
}

namespace Embed {
	void write(std::string_view out) {
		if (out.empty()) return;
		{
			std::lock_guard lock{out_mtx};
			out_buf.append(out);
		}
		wake_host();
	}

	auto width() -> int { return view_width.load(); }
	auto height() -> int { return view_height.load(); }

	bool wait_input(uint64_t timeout_ms, std::string& out) {
		std::unique_lock lock{in_mtx};
		const auto ready = [] { return in_interrupted or not in_queue.empty() or stop_requested.load(); };
		if (timeout_ms == std::numeric_limits<uint64_t>::max()) in_cv.wait(lock, ready);
		else in_cv.wait_for(lock, std::chrono::milliseconds(timeout_ms), ready);
		if (in_interrupted) {
			in_interrupted = false;
			return false;
		}
		if (in_queue.empty()) return false;
		out = std::move(in_queue.front());
		in_queue.pop_front();
		return true;
	}

	void interrupt() {
		{
			std::lock_guard lock{in_mtx};
			in_interrupted = true;
		}
		in_cv.notify_all();
	}

	void request_leave() {
		leave_requested = true;
		wake_host();
	}

	bool visible() {
		std::lock_guard lock{in_mtx};
		return shown;
	}

	void wait_visible() {
		std::unique_lock lock{in_mtx};
		in_cv.wait(lock, [] { return shown or stop_requested.load(); });
	}

	bool stopping() { return stop_requested.load(); }

	auto config_dir() -> std::filesystem::path { return conf_dir; }

	auto agents() -> std::pair<std::unordered_map<size_t, Agent>, uint64_t> {
		std::lock_guard lock{agents_mtx};
		return {agent_table, agent_generation};
	}
}

extern "C" {

int medulla_btop_start(const char* config_dir, uint16_t cols, uint16_t rows,
                       medulla_btop_waker wake, void* wake_ctx) {
	if (started.exchange(true)) return -1;
	conf_dir = config_dir != nullptr ? std::filesystem::path(config_dir) : std::filesystem::path();
	view_width = cols;
	view_height = rows;
	waker = wake;
	waker_ctx = wake_ctx;
	try {
		std::thread(thread_entry).detach();
	}
	catch (const std::system_error&) {
		return -2;
	}
	return 0;
}

void medulla_btop_resize(uint16_t cols, uint16_t rows) {
	if (view_width.exchange(cols) == cols and view_height.exchange(rows) == rows) return;
	Embed::interrupt();
}

void medulla_btop_input(const uint8_t* bytes, size_t len) {
	if (bytes == nullptr or len == 0) return;
	{
		std::lock_guard lock{in_mtx};
		in_queue.emplace_back(reinterpret_cast<const char*>(bytes), len);
	}
	in_cv.notify_all();
}

void medulla_btop_set_visible(int visible) {
	{
		std::lock_guard lock{in_mtx};
		if (shown == (visible != 0)) return;
		shown = visible != 0;
		in_interrupted = true;
	}
	in_cv.notify_all();
}

void medulla_btop_set_agents(const medulla_btop_agent* agents, size_t len) {
	std::unordered_map<size_t, Embed::Agent> table;
	for (size_t i = 0; agents != nullptr and i < len; ++i) {
		table[agents[i].pid] = Embed::Agent{
			agents[i].label != nullptr ? std::string(agents[i].label) : std::string(),
			agents[i].launched != 0,
		};
	}
	std::lock_guard lock{agents_mtx};
	bool same = table.size() == agent_table.size();
	for (auto it = table.begin(); same and it != table.end(); ++it) {
		auto found = agent_table.find(it->first);
		same = found != agent_table.end() and found->second.label == it->second.label
			and found->second.launched == it->second.launched;
	}
	if (same) return;
	agent_table = std::move(table);
	++agent_generation;
}

void medulla_btop_drain(void (*sink)(void*, const uint8_t*, size_t), void* ctx) {
	std::string taken;
	{
		std::lock_guard lock{out_mtx};
		taken.swap(out_buf);
	}
	if (sink != nullptr and not taken.empty())
		sink(ctx, reinterpret_cast<const uint8_t*>(taken.data()), taken.size());
}

int medulla_btop_take_leave(void) {
	return leave_requested.exchange(false) ? 1 : 0;
}

size_t medulla_btop_error(char* buf, size_t cap) {
	std::lock_guard lock{err_mtx};
	if (buf != nullptr and cap > 0) {
		const size_t n = std::min(cap - 1, fatal_message.size());
		std::memcpy(buf, fatal_message.data(), n);
		buf[n] = '\0';
	}
	return fatal_message.size();
}

void medulla_btop_stop(uint32_t timeout_ms) {
	if (not started.load()) return;
	stop_requested = true;
	in_cv.notify_all();
	Embed::interrupt();
	std::unique_lock lock{done_mtx};
	done_cv.wait_for(lock, std::chrono::milliseconds(timeout_ms), [] { return finished.load(); });
}

}
