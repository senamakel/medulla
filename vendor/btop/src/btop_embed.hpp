// SPDX-License-Identifier: Apache-2.0
//
// Medulla embedding seam for btop.
//
// Upstream btop owns a real terminal: it puts stdin in raw mode, reads keys
// with pselect, wakes itself with SIGUSR1, writes escape sequences to stdout and
// ends the process with quick_exit. None of that is acceptable inside a host
// process that owns the terminal itself. This namespace is what the patched
// btop sources call instead:
//
//   - output goes to an in-memory buffer the host drains (Embed::write);
//   - the terminal size is whatever the host last reported (Embed::size);
//   - input arrives as discrete chunks queued by the host (Embed::wait_input);
//   - an interrupt is a condition-variable notify, not a signal;
//   - "quit" asks the host to leave the view (Embed::request_leave), and a
//     fatal error unwinds btop's own thread (Embed::Fatal) instead of exiting.
//
// The C ABI the host links against is in include/medulla_btop.h.

#pragma once

#include <cstdint>
#include <filesystem>
#include <string>
#include <string_view>
#include <unordered_map>
#include <vector>

namespace Embed {

	//* Thrown by clean_quit() on a fatal error; caught at the top of btop's thread.
	struct Fatal {
		std::string message;
	};

	//* Append terminal output for the host to drain, and wake the host.
	void write(std::string_view out);

	//* The host-reported view size in cells.
	auto width() -> int;
	auto height() -> int;

	//* Block up to <timeout_ms> for one input chunk from the host.
	//* Returns false on timeout or on interrupt(), true with <out> filled otherwise.
	bool wait_input(uint64_t timeout_ms, std::string& out);

	//* Wake a thread blocked in wait_input() without delivering input.
	void interrupt();

	//* Record that the operator asked to leave btop (q, or Quit in the menu).
	void request_leave();

	//* Whether the host currently shows btop; collection pauses while hidden.
	bool visible();

	//* Block until the host shows btop again or asks it to stop.
	void wait_visible();

	//* Whether the host asked btop's thread to stop.
	bool stopping();

	//* Directory holding btop.conf, btop.log and user themes.
	auto config_dir() -> std::filesystem::path;

	//* One coding-agent process the host classified.
	struct Agent {
		std::string label;
		bool launched{};
	};

	//* Snapshot of the host's agent table and its change generation.
	auto agents() -> std::pair<std::unordered_map<size_t, Agent>, uint64_t>;
}
