# Parity: desktop app, terminal UI and CLI

The desktop app (GUI), the terminal UI (TUI) and the CLI are three front ends for the same server, so what one can do the others should be able to do. This page says where that is true today.

**Legend.** ✔ offered and checked (by running it, or by a test that exercises it). ✘ not offered. `?` not checked yet. A number is the pull request that added or fixed it.

## Agents

| Function | GUI | TUI | CLI |
|---|---|---|---|
| List agents | ✔ | ✔ | ✔ `agents` |
| Spawn an agent | ✔ | ✔ | ✔ `spawn` |
| Kill an agent | ✔ | ✔ | ✔ `kill` |
| Clean finished agents | ✔ | ✔ | ✔ `clean` |
| Resume after a server restart | ✔ | ✔ `u` (#22) | ✔ `resume` (#21) |
| Reconcile with the terminals | ✘ | ✘ | ✔ `reconcile` (#21) |
| Link an agent to a task | ✔ (#24) | ✘ | ✔ `assign` (#24) |
| Open an agent's terminal | `?` | ✔ | ✔ `attach`, `focus` |
| Skills of one agent (turn on/off, restore) | read only (#17) | ✘ broken, see below | ✘ |

## Snapshots

| Function | GUI | TUI | CLI |
|---|---|---|---|
| Capture, list, show | ✔ (#16) | ✔ | ✔ `snapshot` |
| Restore (with the automatic pre-restore snapshot) | ✔ (#16) | ✔ | ✔ |
| Delete | ✔ (#16) | ✔ | ✔ |
| Prune, diff against the current worktree | ✘ | ✔ | ✔ |

## Tasks

| Function | GUI | TUI | CLI |
|---|---|---|---|
| Create | ✔ | ✔ | ✔ `task new` |
| Edit | ✔ (#24) | ✔ (opens your editor) | ✔ `task edit` |
| Archive | ✔ | ✔ | ✔ `task archive` |
| Delete for good | ✔ (#21) | ✔ `D` (#22) | ✔ `task delete --confirm` (#21) |
| Import a GitHub issue | ✔ (#26) | ✔ (#20) | ✔ `task import` (#21) |

## Runs and gates

| Function | GUI | TUI | CLI |
|---|---|---|---|
| Start a run for a task | via agent or pattern | ✔ | ✔ `run` |
| Cancel a task's run | ✔ (#26) | ✔ (#18) | ✔ `cancel` (#18) |
| Approve a gate | ✔ (#15) | ✔ | ✔ `approve` |
| Reject a gate | ✔ (#15, no reason text yet) | ✔ | ✔ `reject --reason` |

## Patterns (teams)

| Function | GUI | TUI | CLI |
|---|---|---|---|
| List, show, create, edit, validate, delete | ✔ | ✔ | ✔ `pattern …` |
| Start a pattern run | ✔ | ✔ | ✔ `pattern run` |
| Pause, resume, stop a run | ✔ | ✔ | ✔ |
| Delete one run | ✔ (#24) | ✘ | ✔ `pattern delete-run` (#24) |
| Forget finished runs | ✔ | ✔ `C` (#22) | ✔ `pattern clean` (#21) |

## Event bus

| Function | GUI | TUI | CLI |
|---|---|---|---|
| List topics, groups, tail | ✔ | ✔ | ✔ `topic …` |
| Replay | `?` | ✔ | ✔ `topic replay` |
| Create a topic, publish | ✘ | ✘ | ✔ |
| Clear the bus | ✔ | ✔ `C` (#22) | ✔ `bus clean` (#21) |

## Skills

| Function | GUI | TUI | CLI |
|---|---|---|---|
| List the catalog | ✔ | ✔ | ✔ `skills list` |
| Create, delete | ✔ | ✔ (#18) | ✔ (#21) |
| Make global | ✔ (#17) | ✔ (#18) | ✔ `skills global` (#21) |

## Git

| Function | GUI | TUI | CLI |
|---|---|---|---|
| Status of worktrees | ✔ | ✔ | ✔ `git status` |
| Create a branch or a worktree, remove a worktree | ✔ (#30) | ✔ (#19) | ✔ (#21) |
| Push a branch and open its PR | only by approving a gate | ✔ (#20) | ✔ `git publish` (#21) |

`publish` never adds a push path of its own: it approves the gate of the subtask waiting on that branch, and refuses a branch with no gate waiting.

## Known gaps

- **Per-agent skills.** The TUI sends `skills_toggle` and `skills_restore`, but the server has no such operations, so those two actions fail with "unknown shepherd operation". What they should do to an agent that is already running (change its installed copy now, or only apply at its next start) is not decided.
- **GUI:** a reason when rejecting a gate (#29), snapshot prune and diff against the current worktree, and the bus-topic create/publish.
- **TUI:** linking an agent to a task, deleting a single pattern run, creating a topic or publishing to it.

## How this is kept true

One test, `every_operation_a_surface_sends_is_known_to_the_server` (in `shepherd-core`), reads the operations the TUI and the app send and calls each one on the server. It fails when a button sends something the server does not know, which is how the gaps above were found. The two per-agent skill operations are listed in it as known gaps; implementing one makes the test ask you to take it off the list.

The CLI has a test that every command it routes appears in `shepherd --help`, and that every command of the parity work is routed (a command missing from the router answers "unknown command").
