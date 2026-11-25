# The pieces of Shepherd

Shepherd has a handful of pieces. Each does one thing, and you create or change only a few of them. This page says what each piece is, what it is for and what you do with it. It ends with how to tell the three that people confuse most (role, skill and pattern) apart.

## 1. How they fit

A **pattern** names the agents of a team and the **topics** they talk through. Each **agent** has a **role**, and the role brings a vendor and model, an optional own **worktree**, and **skills**. Starting the pattern is a **run**: agents start, work on their own branches, and hand results to each other through the topics. A **check** decides whether the work is good. **Snapshots** let you undo. You review the branches and merge them yourself.

```
 pattern ──► agents ──► role ──► vendor + model + skills
    │           │
    │           └──► own worktree and branch ──► you review and merge
    └──► topics ◄──► agents talk to each other      check = the project's real tests
```

## 2. What each piece is for

### Workspace

- **What it is:** A group of tabs and terminal panes for one project.
- **What it is for:** Keeps every agent and terminal of a repository together.
- **What you do:** Create one per repository, switch between them, close the ones you are done with.
- **Where:** The sidebar of the app or the TUI; `shepherd workspace ...`

### Agent

- **What it is:** A coding CLI (claude, codex, agy) running in its own terminal pane, with an id such as `coder-1` and a state (`starting`, `working`, `idle`, `blocked`, `done`).
- **What it is for:** Does one job: write code, review a diff, run tests.
- **What you do:** Start one, watch it, open its terminal to answer a prompt, kill it, clean it up.
- **Where:** Agents view; `shepherd spawn`, `agents`, `attach`, `kill`, `clean`

### Role

- **What it is:** A named job: which vendor and model fill it by default, whether it gets its own worktree, whether it may start other agents, which skills it carries and an optional prompt.
- **What it is for:** Says **who** does a job, once, so you can ask for it by name (`coder`, `reviewer`, or your own `gopher`).
- **What you do:** Use the five built-in roles, change one, or add your own.
- **Where:** `config.yaml`; Models and Agents views; `shepherd roles list`

### Vendor and model

- **What it is:** Which agent CLI runs (claude, codex, gemini through agy) and which model it uses.
- **What it is for:** Lets you spread work across vendors and pay less where the job is small.
- **What you do:** Leave the role's default, or override it for one agent with `--vendor` and `--model` (or `vendor` and `model` on a pattern agent).
- **Where:** `shepherd vendors list`; Models view

### Skill

- **What it is:** A folder with a `SKILL.md`: a short manual whose one-line description starts with "Use when...". The agent loads it when the description fits its work.
- **What it is for:** Teaches **how** your project does things (conventions, commands, checks) once, for every agent that needs it.
- **What you do:** Write one, keep it in the repo, assign it globally, to a role or to one agent, and turn it off where it does not belong.
- **Where:** Skills view; `shepherd skills create`, `skills agent`; `.shepherd/skills/`

### Pattern

- **What it is:** A team described in a YAML file: its agents, the topics they talk through, and who starts.
- **What it is for:** Says **who talks to whom**, so several agents can work on one objective in a known way.
- **What you do:** Duplicate a built-in one, edit it, validate it, run it with an objective.
- **Where:** Patterns view; `shepherd pattern new`, `validate`, `run`; `.shepherd/patterns/`

### Topic

- **What it is:** A named stream of messages that agents publish to and read from. It can have partitions, consumer groups and a hard cap (`max_messages`).
- **What it is for:** How agents hand work and results to each other.
- **What you do:** Declare the topics a pattern needs, give the agents `publishes` and `consumes`, and watch the messages as they flow.
- **Where:** Events view; `shepherd topic list`, `tail`, `replay`

### Run

- **What it is:** One execution of a pattern with an objective.
- **What it is for:** Turns the team description into agents that are working.
- **What you do:** Start it, watch its state, pause, resume or stop it, delete it when it is over.
- **Where:** Runs view; `shepherd pattern run`, `runs`, `pause`, `resume`, `stop`

### Worktree and branch

- **What it is:** A separate folder of your repository on its own branch (`shep/<n>-...`). A role with `worktree: true` gives each of its agents one.
- **What it is for:** Agents cannot overwrite each other or your checkout.
- **What you do:** Look at them, read the diff, merge the branch yourself with git, remove the worktree.
- **Where:** Git view; `shepherd git status`, `git worktree remove`

### Check

- **What it is:** The project's own commands that say whether the work is good: `workflow.checks`, or the ones Shepherd detects (`go test ./...`, `cargo test`, `pytest`, `npm test`).
- **What it is for:** Gives reviewers facts instead of opinions: pass or fail is the exit code.
- **What you do:** List your checks once in `config.yaml`; agents call the `run_checks` tool and a task run executes them before it publishes.
- **Where:** `config.yaml`; the `run_checks` MCP tool

### Task

- **What it is:** A unit of work in `.shepherd/tasks` with a specification and acceptance criteria.
- **What it is for:** Structured work with a plan, a review and gates where Shepherd waits for you.
- **What you do:** Create it, run it (`shepherd run`), approve or reject the plan and each publish.
- **Where:** Tasks view; `shepherd task new`, `run`, `approve`, `reject`

### Snapshot

- **What it is:** A copy of an agent's worktree that Shepherd takes on its own and on request, kept in `refs/shepherd/snapshots/`.
- **What it is for:** Lets you undo a bad edit without touching your branch.
- **What you do:** Compare a snapshot with the current state and restore all of it or one file.
- **Where:** Snapshots view; `shepherd snapshot ...`

## 3. Role, skill or pattern?

The three answer different questions:

| | Answers | Lives in | Example |
|---|---|---|---|
| **Role** | *Who* does the job | `roles:` in `config.yaml` | `gopher`: claude haiku, own worktree |
| **Skill** | *How* it is done here | `.shepherd/skills/<name>/SKILL.md` | "Use when writing Go HTTP handlers..." |
| **Pattern** | *Who talks to whom* | `.shepherd/patterns/<name>.yaml` | a generator and a critic on one topic |

Which one do I need?

| Situation | Use | For example |
|---|---|---|
| Say who does a job (which vendor and model, own branch or not) | **Role** | `coder` on claude instead of codex, or a new `gopher` |
| Make every agent that touches this code follow your conventions | **Skill** | "Use when writing Go HTTP handlers..." |
| Have several agents work together on one objective | **Pattern** | a generator and a critic that correct each other |
| Get one agent to do one thing now | **Agent** (`shepherd spawn`) | `shepherd spawn coder "Add GET /health"` |
| Do structured work with a plan and approvals | **Task** (`shepherd run`) | an issue that becomes subtasks, review and a PR |

They stack: a skill goes on a role, a role fills an agent, and a pattern is a team of agents. The recipe [your own role, skill and pattern](recipe-own-team.md) creates one of each.

## 4. Where to go next

- [Agents, roles, vendors and skills](agents-roles-skills.md): every field of a role, the three skill levels.
- [Patterns](patterns.md): the YAML format and the ten built-in patterns, one by one.
- [Topics and the event bus](topics-and-events.md): partitions, consumer groups, watching messages.
- [Your own role, skill and pattern](recipe-own-team.md): a worked example that builds a Go server.
