/* SPDX-License-Identifier: Apache-2.0
 *
 * C ABI for running btop inside Medulla.
 *
 * btop runs on its own thread inside the host process. The host feeds it a
 * view size and input bytes, and drains the ANSI output btop would otherwise
 * have written to a terminal. Every function is safe to call from any thread.
 *
 * btop keeps its state in process globals, so it can be started at most once
 * per process: medulla_btop_start() after a stop, or a second start, fails.
 */

#ifndef MEDULLA_BTOP_H
#define MEDULLA_BTOP_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Called (from btop's threads) whenever new output is ready to drain. */
typedef void (*medulla_btop_waker)(void *ctx);

/* One coding-agent process, as classified by the host. */
typedef struct {
	uint32_t pid;
	/* Short name shown in the Agent column, UTF-8, NUL-terminated. */
	const char *label;
	/* Non-zero when the host itself launched this process. */
	uint8_t launched;
} medulla_btop_agent;

/* Start btop's thread. <config_dir> holds btop.conf and is created if absent.
 * Returns 0 on success, -1 if btop was already started in this process, -2 if
 * the thread could not be created. */
int medulla_btop_start(const char *config_dir, uint16_t cols, uint16_t rows,
                       medulla_btop_waker waker, void *waker_ctx);

/* Report a new view size in cells. */
void medulla_btop_resize(uint16_t cols, uint16_t rows);

/* Queue one key or mouse event, encoded as the terminal bytes a real terminal
 * would send (e.g. "\x1b[A", "\x1b[<0;12;4M"). */
void medulla_btop_input(const uint8_t *bytes, size_t len);

/* Show or hide btop. While hidden, btop stops collecting and drawing; on show
 * it redraws everything. */
void medulla_btop_set_visible(int visible);

/* Replace the agent table. Strings are copied. */
void medulla_btop_set_agents(const medulla_btop_agent *agents, size_t len);

/* Copy and clear pending output. Calls <sink> with the bytes when any exist. */
void medulla_btop_drain(void (*sink)(void *ctx, const uint8_t *bytes, size_t len), void *ctx);

/* Returns 1 once per operator request to leave btop, else 0. */
int medulla_btop_take_leave(void);

/* Copies a fatal error message, if btop's thread died, into <buf> (truncated
 * and NUL-terminated). Returns the full message length, or 0 when running. */
size_t medulla_btop_error(char *buf, size_t cap);

/* Ask btop's thread to stop, save its config, and wait up to <timeout_ms>. */
void medulla_btop_stop(uint32_t timeout_ms);

#ifdef __cplusplus
}
#endif

#endif /* MEDULLA_BTOP_H */
