# Keyboard shortcuts and splitting the terminal

Shepherd is mouse-native: you can click panes, tabs and workspaces, drag the borders between split panes and use right-click menus without learning a single key. The keyboard is optional. This page lists every default key, how to split the terminal, and the CLI command behind each action.

## 1. Where each set of keys works

| Where | Keys | See |
|---|---|---|
| Terminal UI (`shepherd --tui`) | The **prefix keys** (`ctrl+b` then a letter): split, move, zoom and resize panes, tabs, workspaces, copy mode. | Sections 2 to 8 |
| Shepherd section of the terminal UI | Press `prefix+a`, then digits `1` to `7` switch tabs and each tab has its own letters. | Section 10 |
| Desktop app (`shepherd --gui`) | No prefix. `Ctrl+Shift+P` command palette, `Ctrl+B` sidebar, `Ctrl+1`..`Ctrl+9` views, `Ctrl+Shift+S` Terminal or Shepherd. The desktop app does not split panes: use the terminal UI or `shepherd pane split`. | Section 11 |
| CLI | Every action has a command that talks to the same server. | Section 9 |

## 2. The prefix

A terminal multiplexer sits between your terminal and the programs inside it, and those programs already use most key combinations (`ctrl+c` interrupts, `ctrl+r` searches history, editors claim the rest). So Shepherd reserves one key, the **prefix**, `ctrl+b` by default: press it, release it, and the next key goes to Shepherd instead of the program. `prefix+c` means: press `ctrl+b`, release, press `c`.

Press `prefix+?` at any time to see every active binding; `/` filters them.

## 3. Learn these five first

| Action | Key |
|---|---|
| New tab | `prefix+c` |
| Split right / down | `prefix+v` / `prefix+minus` |
| Move between panes | `prefix+h` `j` `k` `l` |
| Workspace picker | `prefix+w` |
| Detach and leave everything running | `prefix+q` |

They cover most daily movement. Everything else can stay on the mouse.

## 4. Panes: split, move, zoom, resize

| Action | Key |
|---|---|
| Split right (side by side) | `prefix+v` |
| Split down (one above the other) | `prefix+minus` |
| Move to the pane on the left, below, above, right | `prefix+h` / `j` / `k` / `l` |
| Next / previous pane | `prefix+tab` / `prefix+shift+tab` |
| Zoom the focused pane (again to restore) | `prefix+z` |
| Resize mode (`prefix+?` shows its keys) | `prefix+r` |
| Swap the pane with its neighbour | `prefix+shift+h` / `j` / `k` / `l` |
| Close the pane | `prefix+x` |
| Rename the pane | `prefix+shift+p` |
| Copy mode (scroll back, search, select) | `prefix+[` |
| Open the scrollback in your editor | `prefix+e` |

```
 prefix+v                prefix+minus
 ┌────────┬────────┐      ┌─────────────────┐
 │        │        │      │                 │
 │  pane  │  new   │      ├─────────────────┤
 │        │        │      │       new       │
 └────────┴────────┘      └─────────────────┘
```

With the mouse: click a pane to focus it, drag a border to resize it, right-click for the menu.

## 5. Tabs

| Action | Key |
|---|---|
| New tab | `prefix+c` |
| Next / previous tab | `prefix+n` / `prefix+p` |
| Jump to tab 1 to 9 | `prefix+1` .. `prefix+9` |
| Rename the tab | `prefix+shift+t` |
| Close the tab | `prefix+shift+x` |

## 6. Workspaces and the session

| Action | Key |
|---|---|
| Workspace picker | `prefix+w` |
| Goto picker: every agent and terminal, grouped by workspace | `prefix+g` |
| New workspace | `prefix+shift+n` |
| New worktree | `prefix+shift+g` |
| Rename / close the workspace | `prefix+shift+w` / `prefix+shift+d` |
| Show or hide the sidebar | `prefix+b` |
| Open the Shepherd section (agents, tasks, runs, patterns...) | `prefix+a` |
| Settings | `prefix+s` |
| Open the target of the latest notification | `prefix+o` |
| Reload the config | `prefix+shift+r` |
| Show every active binding (`/` filters) | `prefix+?` |
| Detach, leaving every agent running (run `shepherd --tui` to come back) | `prefix+q` |

In the Goto picker (`prefix+g`) every agent or terminal has its own row, grouped by workspace. Press `/` to search names, agent kinds, tabs, workspaces, branches or paths; `Enter` jumps to the row. Outside the search, `b` `w` `i` `d` filter blocked, working, idle or done agents and `a` shows everything again.

## 7. Copy mode

Press `prefix+[` to enter copy mode for the focused pane. The pane keeps running while you read its history.

| To | Keys |
|---|---|
| Move | `h` `j` `k` `l`, words `w` `b` `e` (big words `W` `B` `E`), paragraphs `{` `}`, `PageUp` `PageDown`, `ctrl+u` `ctrl+d` |
| Search | `/` forward, `?` backward, then `n` / `N` to repeat. Case-insensitive unless the query has a capital letter |
| Select and copy | `v` or `Space` starts a selection, `y` or `Enter` copies it |
| Leave | `q` or `Esc` (the first `Esc` clears a selection or a search) |

Dragging with the mouse copies without entering copy mode.

## 8. Editing text fields

The naming dialogs, branch fields and filters of Shepherd edit at the cursor. These keys apply to Shepherd's own fields, not to the shell or agent inside a pane.

| Action | Keys |
|---|---|
| Move one character | `Left` / `Right`, `ctrl+b` / `ctrl+f` |
| Start / end of the line | `Home` / `End`, `ctrl+a` / `ctrl+e` |
| Move one word | `alt+b` / `alt+f` |
| Delete the previous / next character | `Backspace`, `ctrl+h` / `Delete`, `ctrl+d` |
| Cut to the start / end | `ctrl+u` / `ctrl+k` |
| Cut the previous / next word | `ctrl+w`, `alt+Backspace` / `alt+d` |
| Paste the last cut text | `ctrl+y` |

## 9. The same actions from the CLI

Every key calls the same server operation as a CLI command, so you can split panes and open tabs from a script or from another terminal:

| Action | Command |
|---|---|
| Split a pane | `shepherd pane split --direction right\|down [--current] [--cwd PATH]` |
| Move focus | `shepherd pane focus --direction left\|right\|up\|down` |
| Zoom | `shepherd pane zoom [PANE_ID]` |
| Resize | `shepherd pane resize --direction left\|right\|up\|down` |
| Swap | `shepherd pane swap --direction left\|right\|up\|down` |
| Close a pane | `shepherd pane close PANE_ID` |
| New tab / focus / rename / close | `shepherd tab create [--workspace ID] [--cwd PATH] [--label TEXT]`, `tab focus ID`, `tab rename`, `tab close` |
| Workspaces | `shepherd workspace list`, `create`, `focus`, `rename`, `close` |
| List panes and read one | `shepherd pane list`, `shepherd pane read PANE_ID` |

## 10. The Shepherd section of the terminal UI

Run `shepherd --tui` in your repository and press `prefix+a` to open the Shepherd section (`Esc` leaves it). Digits `1` to `7` switch tabs: Agents, Tasks, Runs, Patterns, Skills, Git, Events. The bottom line always shows the keys of the tab you are on. Actions that destroy something show a confirmation line: `Enter` confirms, `Esc` cancels.

### 1 Agents

| Key | Action |
|---|---|
| `Up` `Down` | Select an agent |
| `Enter` | Open its terminal |
| `s` | New agent (role, task, vendor and model, worktree, extra skills) |
| `l` | Link the agent to a task, or detach it |
| `t` | Snapshots of the agent |
| `K` | Kill (asks first) |
| `x` / `X` | Clean this finished agent / all finished ones (asks first) |
| `r` | Refresh |
| `u` | Resume: mark the agents a server restart left behind (starts nothing) |

### 2 Tasks

| Key | Action |
|---|---|
| `n` | New |
| `e` | Edit (opens your editor) |
| `i` | Import a GitHub issue |
| `a` | Archive |
| `D` | Delete for good (asks first) |

### 3 Runs

| Key | Action |
|---|---|
| `l` | Start a run for the selected task |
| `c` | Cancel it |
| `a` | Approve the waiting gate |
| `d` | Reject it |

### 4 Patterns

| Key | Action |
|---|---|
| `n` | New |
| `e` | Edit |
| `c` | Duplicate |
| `v` | Validate |
| `d` | Delete |

### 5 Skills

| Key | Action |
|---|---|
| `c` | Switch between the catalog and by agent |
| `n` `g` `d` | Catalog: new, make global, delete |
| `Space` `r` | By agent: turn on or off, restore |

### 6 Git

| Key | Action |
|---|---|
| `w` | New worktree |
| `b` | New branch |
| `p` | Publish (approves the subtask waiting on that branch) |
| `t` | Snapshots |
| `x` | Delete a worktree |
| `c` | Copy the path |

### 7 Events

| Key | Action |
|---|---|
| `Tab` | Switch between topics and pattern runs |
| `Space` `r` `/` | Pause, replay from an offset, filter |
| `n` `m` | New topic, message for the selected topic |
| `C` | Clear the bus (asks first) |
| `p` `x` `D` `C` | Pattern runs: pause or resume, stop, delete, forget finished |

## 11. The desktop app

The desktop app has no prefix. It works with the mouse, the command palette and these keys:

| Keys | Action |
|---|---|
| `Ctrl+Shift+S` | Switch between Terminal and Shepherd |
| `Ctrl+Shift+P` | Open the command palette (type to filter, `Up` `Down`, `Enter` runs, `Esc` closes) |
| `Ctrl+B` | Show or hide the sidebar |
| `Ctrl+1` .. `Ctrl+5` | Workspace views: Overview, Runs, Agents, Tasks, Patterns |
| `Ctrl+6` .. `Ctrl+9` | Inspect views |

The app does not split panes. To split the terminal use the terminal UI (`shepherd --tui`) or `shepherd pane split`.

## 12. Change any key

Every binding is configurable, including the prefix, in the `[keys]` table of `config.toml`:

```toml
[keys]
prefix = "ctrl+a"                      # a different prefix
split_vertical = ["prefix+v", "ctrl+alt+d"]   # several keys for one action
focus_pane_left = ["prefix+h", "ctrl+alt+h"]
```

- `shepherd --default-config` prints the whole commented default config, with every action and its default key.
- `shepherd config reset-keys` backs up `config.toml` and goes back to the built-in keys.
- Plain printable keys such as `n` are unsafe as direct bindings because they steal typing; bind them with `prefix+`. For direct chords, `ctrl+alt+letter` is the safest family: terminals and desktops leave it free. Avoid `ctrl+alt+arrows`, `ctrl+alt+t`, `ctrl+alt+l`, `ctrl+alt+s`, `ctrl+alt+u` and `ctrl+alt+f1..f12`, which desktops and terminals use. If a direct chord does nothing, your terminal or desktop consumed it first.
