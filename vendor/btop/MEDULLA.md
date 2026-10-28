# btop, vendored for Medulla

Upstream: [aristocratos/btop](https://github.com/aristocratos/btop) **v1.4.8**,
Apache-2.0 (`LICENSE`). Imported unmodified in the first commit that adds this
directory, so `git diff <that commit> -- vendor/btop` is exactly the list below.

Kept: `src/` shared sources, the Linux and macOS collectors, and the bundled
`include/fmt` and `include/widechar_width.hpp`. Dropped: the CLI (`btop_cli.*`,
`main.cpp`), the BSD collectors, GPU support (`intel_gpu_top`, NVML/ROCm), the
CMake/Make build, themes, docs and tests. `src/btop/build.rs` (the
`medulla-btop` crate) is the build.

## Embedding: btop without a terminal of its own

btop runs on a thread inside Medulla. Everything that assumed it owned the
process's terminal goes through `src/btop_embed.{hpp,cpp}` instead, with the C
ABI in `include/medulla_btop.h`.

| Upstream | Embedded |
|---|---|
| `btop_main()` parses argv, sets up the tty and signal handlers, loops | `btop_embed_main()` (end of `btop.cpp`): same scheduling loop, no argv, no signals, pauses while the host hides it |
| `cout << …` | `Embed::write()` into a buffer the host drains |
| `Term::init/refresh/restore` use `tcsetattr`/`TIOCGWINSZ` | size comes from `medulla_btop_resize()`; no tty is touched |
| `Input::poll` reads stdin with `pselect` | `Embed::wait_input()` pops one host-queued key chunk |
| `Input::interrupt` sends `SIGUSR1` to the process | condition-variable notify |
| `q` / menu Quit / size-error `q` call `clean_quit(0)` → `quick_exit` | `Embed::request_leave()`; the host leaves the tab |
| `clean_quit(1)` on fatal errors | throws `Embed::Fatal`, caught on btop's thread; the host shows the message |
| `⌃R` sends `SIGUSR2` | sets `Global::reload_conf` |
| config/log in `$XDG_CONFIG_HOME/btop`, `$XDG_STATE_HOME` | `<config_dir>/btop.conf`, `btop.log`, `themes/` chosen by the host |
| `setlocale(LC_ALL, …)` | `LC_CTYPE` only — the locale is the host's too |
| `Mv::to` emits HVP (`CSI y;x f`), `Mv::save/restore` emit `CSI s/u` | CUP (`CSI y;x H`) and DECSC/DECRC (`ESC 7/8`): the host's `vt100` emulator implements those, not the former |

## Agent focus

- `proc_agents` (config, default **on**; `A` toggles, also in Options): the
  process box shows only the coding-agent processes the host names through
  `medulla_btop_set_agents()` and their descendants, titled `agents`. In tree
  view each agent is a root.
- In that mode the `User` column is `Agent`: the agent's name (`◆` and the
  highlight colour when Medulla launched it) on its own process, `↳ name`
  dimmed on its descendants. An empty box says why and how to see everything.
- `Proc::refresh_agents/passes_filter/agent_of/box_title` (`btop_shared.cpp`)
  hold the logic; both collectors call it where they apply the text filter.
- Default `proc_tree` is **on**.

## Updating upstream

Import the new release over a scratch branch, commit it unmodified, then replay
the diff above. The embed seam touches few lines per file by design; the agent
hooks are the two-line changes in each `btop_collect.cpp` filter block.
