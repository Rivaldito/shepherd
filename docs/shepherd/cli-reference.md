# CLI reference

Every `shepherd` command, grouped by what it does.

`shepherd <command> [--json]`. `--json` prints the raw answer and is what scripts should use. `--session <name>` talks to a named session instead of the default. `shepherd --help` lists everything below, and `shepherd snapshot --help` has the snapshot options.

Commands that destroy something ask for `--confirm` or `--force`. An unknown agent or task answers `shepherd: unknown agent "x"` with exit code 1.

## Interface, server and sessions

| Command | What it does |
|---|---|
| `shepherd` | Open the desktop app, starting the server first. Where the app cannot open (no screen, or not installed) it opens the terminal UI |
| `shepherd --gui` | Open the desktop app, or say why it cannot and point to `--tui` |
| `shepherd --tui` | Open the terminal UI (the multiplexer with the Shepherd section). `SHEPHERD_NO_GUI=1 shepherd` does the same without the flag |
| `shepherd --session <name>` | Use or create a named persistent session, a separate server with its own state |
| `shepherd session list`, `attach <name>`, `stop <name>`, `delete <name>` | Manage named sessions |
| `shepherd --remote <ssh-target>` | Attach through SSH to a Shepherd server on another machine |
| `shepherd --machine <label>` `<command>` | Run an API command on a saved SSH machine |
| `shepherd machine list`, `status`, `reconnect`, `add`, `rename`, `remove`, `enable`, `disable` | Manage saved SSH machines |
| `shepherd status [server\|client]` | Show the running server and the local client |
| `shepherd server stop`, `restart`, `reload-config` | Stop the server, replace it with a new one (closes its terminals and agents), or reload `config.toml` |
| `shepherd server agent-manifests`, `update-agent-manifests`, `reload-agent-manifests` | Show, fetch or reload the agent detection manifests |
| `shepherd update [--handoff]` | Download and install the latest version |
| `shepherd channel show`, `channel set stable\|preview` | Choose the update channel |
| `shepherd integration install\|uninstall\|status` | Manage the built-in agent integrations |
| `shepherd completion <shell>` | Print shell completions (for example `zsh`) |
| `shepherd --default-config` | Print the whole commented default `config.toml`, with every key and its default |
| `shepherd config check`, `config reset-keys` | Validate `config.toml`, or back it up and go back to the built-in keybindings |
| `shepherd --skill`, `--shepherd-skill` | Print the skill file for agents, or the Shepherd orchestration skill |

## Terminal control: workspaces, tabs, panes and agents

These talk to the same server over its socket and are what the terminal UI and the desktop app are built on. Panes are named like `wA:p1`; `--current` means the pane you are in. The key bindings of the terminal UI call these same operations; see [keyboard shortcuts](shortcuts.md).

| Command | What it does |
|---|---|
| `workspace list`, `create`, `get`, `focus`, `rename`, `close` | Workspaces |
| `worktree list`, `create`, `open`, `remove` | Git worktrees opened as workspaces |
| `tab list`, `create [--workspace ID] [--cwd PATH] [--label TEXT]`, `get`, `focus`, `rename`, `close` | Tabs |
| `pane list`, `current`, `get`, `layout`, `process-info`, `neighbor`, `edges` | Look at panes and their layout |
| `pane split --direction right\|down [PANE_ID] [--cwd PATH] [--focus\|--no-focus]` | Split a pane |
| `pane focus`, `resize`, `swap` with `--direction left\|right\|up\|down` | Move focus, resize a split, swap with a neighbour |
| `pane zoom`, `move`, `rename`, `close PANE_ID`, `input` | Zoom, move, rename or close a pane; set its input routing |
| `pane read`, `send-text`, `send-keys`, `wait-output`, `run` | Read a pane's output, type into it, wait for output, run a command in it |
| `agent list`, `get`, `focus`, `rename` | The agents the terminal knows (detected from the panes) |
| `agent read`, `send-keys`, `prompt`, `wait`, `start`, `attach`, `explain` | Read an agent's terminal, press keys, submit a prompt, wait for a state, start a supported CLI in a pane, attach, or explain how its state was detected |
| `notification show` | Show a notification |
| `api snapshot`, `api schema` | Print the live session snapshot or the bundled API schema |

## Agents

| Command | What it does |
|---|---|
| `agents` | List agents |
| `spawn <role> <task...> [--task-id <id>] [--vendor <name>] [--model <name>] [--mcp-url <url>]` | Start an agent (this is real) |
| `attach [agent-or-pane] [--takeover]` | Open an agent's terminal |
| `focus <agent>` | Focus an agent's pane |
| `kill <agent>` | Close an agent |
| `clean [<agent>...] [--workspace <name>]` | Remove finished or killed agents |
| `assign <agent> <task-id\|none>` | Link an agent to a task, or detach it |
| `resume` | Mark the agents a server restart left behind. Starts nothing |
| `reconcile` | Check the agents against their terminals now |

## Tasks and runs

| Command | What it does |
|---|---|
| `task new <title> [--description <text>] [--criteria <text>]... [--id <n>]` | Create a task in `.shepherd/tasks/` |
| `task list [--archived]`, `task show <id> [section]`, `task path <id>` | Read tasks |
| `task plan <id> [--save]` | Have the planner split a task into subtasks |
| `task edit <id> [section]` | Edit in your editor |
| `task archive <id>` | Hide a task (keeps it) |
| `task delete <id> [--archived] --confirm` | Remove a task's folder for good |
| `task import <issue-number-or-url>` | Create a task from a GitHub issue (needs `gh`) |
| `run <task-id> [--mcp-url <url>] [--auto-push] [--wait]` | Start the task workflow: plan, code, review, checks |
| `runs list` | The workflow's runs |
| `cancel <task-id>` | Stop a task's run: closes its agents, marks it cancelled; committed work stays on its branches |
| `approve <task-id> plan\|<subtask>` | Approve a gate |
| `reject <task-id> plan\|<subtask> [--reason <text>]` | Reject a gate |

## Patterns (teams)

| Command | What it does |
|---|---|
| `pattern list`, `show <name>`, `edit <name>`, `delete <name>` | Manage patterns |
| `pattern new <name> --from <template\|name> [--global]` | Copy a template or a pattern |
| `pattern validate <name\|--file path>` | Check it before running |
| `pattern run <name> [--task <task>] [--worktree <path>] [--confirm-over-limit]` | Start a run (starts real agents) |
| `pattern runs`, `pause <id>`, `resume <id>`, `stop <id> [--confirm]` | Watch and control runs |
| `pattern clean` | Forget finished runs |
| `pattern delete-run <id> [--confirm]` | Remove one run (an active one is stopped first, which needs `--confirm`) |

## Event bus

| Command | What it does |
|---|---|
| `topic list`, `topic groups [topic]` | Topics and their consumer groups |
| `topic create <name> [--partitions <n>]` | Create a topic |
| `topic publish <topic> <content> [--key <key>] [--partition <p>] [--from <from>]` | Publish a message |
| `topic tail <topic> [--partition <p>] [--limit <n>]` | Follow a topic |
| `topic replay <topic> [--partition <p>] [--from <offset>] [--limit <n>]` | Read from an offset |
| `bus clean [--force]` | Delete every message of the bus |

## Snapshots

| Command | What it does |
|---|---|
| `snapshot capture <agent> [--reason ...]` | Take one now |
| `snapshot list [--agent <agent>] [--all]`, `show <id> [--full]` | Read them |
| `snapshot diff <id> [<id>] [--full]` | Against the previous, a second one, or the current worktree |
| `snapshot restore <id> [--path <file>]... [--confirm] [--delete-extra]` | Put it back (saves `pre-restore` first) |
| `snapshot delete <id>` | Delete one |
| `snapshot prune [--agent <agent>] [--keep <n>]` | Keep the newest ones |

## Git

| Command | What it does |
|---|---|
| `git status` | Worktrees, their state and agents |
| `git branch <name> [--base <ref>]` | Create a branch without switching to it |
| `git worktree add <path> --branch <name>` | Add a worktree (the branch is created if it does not exist) |
| `git worktree remove <path>` | Remove one; refuses the main checkout, a dirty worktree and one a live agent works in |
| `git publish <branch>` | Push and open the PR of a subtask that waits for your approval; refuses a branch with no gate waiting |

## Skills, roles, config

| Command | What it does |
|---|---|
| `skills list` | The catalog |
| `skills create <name> [--description <text>] [--repo] [--global]` | New skill (for you, or for the project with `--repo`) |
| `skills delete <name>` | A built-in skill cannot be deleted |
| `skills global <name> [--off]` | Send a skill to every agent, or stop |
| `skills agent <agent> <skill> on\|off\|default` | Change one agent's skills; applies at its next start |
| `roles list`, `vendors list` | Roles and vendors |
| `config path`, `config validate`, `config enable` | The config file |
| `mcp serve [port]` | Serve the MCP tools |

See [the recipes](https://shepherd.rivaldito.com/docs/recipes/) for these commands in use.
