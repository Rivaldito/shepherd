# Patterns: multi-agent teams as YAML

A **pattern** is a team of agents described in a YAML file: who takes part, with which role, vendor and model, which topics they use to talk to each other, and who starts. A **run** is one execution of a pattern with an objective.

Patterns turn "open six terminals and copy text between them" into a file you can version, validate and rerun.

## 1. Where patterns live

The first match by name wins:

1. `<repo>/.shepherd/patterns/<name>.yaml`: project, versioned with your code.
2. `~/.config/shepherd/patterns/<name>.yaml`: user-wide (`pattern new --global`).
3. Built-in templates: the four topologies (`orchestrator-worker`, `hierarchical`, `blackboard`, `market`) and six for testing (see [the built-in patterns, one by one](#4b-the-built-in-patterns-one-by-one)). They are read-only starting points.

```
$ shepherd pattern list
NAME                     KIND                 SOURCE   SHADOWED  AGENTS  TOPICS
agentcoder               blackboard           builtin  no        3       1
blackboard               blackboard           builtin  no        3       1
hierarchical             hierarchical         builtin  no        7       4
market                   market               builtin  no        6       3
mutation-testing         blackboard           builtin  no        3       1
orchestrator-worker      orchestrator-worker  builtin  no        4       2
planner-workers-reviewer orchestrator-worker  builtin  no        5       3
reflection-loop          blackboard           builtin  no        2       1
self-debug               blackboard           builtin  no        2       1
tdd                      blackboard           builtin  no        3       1
```

`SHADOWED` tells you when a project or user pattern hides another one with the same name.

## 1b. Which built-in to pick

Every built-in explains itself in the apps and in `shepherd pattern show <name>`: what it is, when to use it and an example. In short:

| Template | Use it when | Example |
|---|---|---|
| `orchestrator-worker` | A job splits into independent pieces of similar size (one module, one handler or one file per worker) and one agent can plan them. Not when the pieces depend on each other (use hierarchical) or when you want rival answers (use market). | `shepherd pattern run orchestrator-worker --task "Add input validation to the 12 API handlers, one handler per worker"` |
| `hierarchical` | The job is big enough to need levels: a director, a manager per area and specialists under each (frontend, backend, tests). Not for small jobs, where the extra layers only cost tokens. | `shepherd pattern run hierarchical --task "Move the billing service to the new payments API: backend, API client and tests teams"` |
| `blackboard` | The problem is unclear and several viewpoints must read each other's findings, with no fixed order: research, hunting a root cause, reviewing a design. Not when the work splits cleanly into parts. | `shepherd pattern run blackboard --task "Find out why the nightly import takes four hours"` |
| `market` | There are several ways to solve it and you want rival proposals: solvers bid, the market-maker picks the winner and that solver does the work. Costs more than one agent, so use it when the choice matters. | `shepherd pattern run market --task "Choose and implement the fastest way to deduplicate 10 million rows"` |
| `reflection-loop` | One agent's output needs a second pair of eyes before you accept it, new code or new tests, and the standard is clear enough for a critic to judge. Not for open exploration. | `shepherd pattern run reflection-loop --task "Write unit tests for src/billing/invoice.rs; the critic must approve before I read them"` |
| `tdd` | You have a clear spec and want tests that do not mirror the implementation: the test writer never sees the code. Not when the spec is still vague. | `shepherd pattern run tdd --task "Implement slugify(title) as described in docs/slugify.md: lowercase, hyphens, no accents"` |
| `agentcoder` | You want an independent test suite that includes edge and large cases, and a real test run whose errors go straight back to the programmer. Needs a project whose tests run from the command line. | `shepherd pattern run agentcoder --task "Implement an LRU cache with a capacity and a time to live"` |
| `mutation-testing` | Tests already exist and you doubt they would catch a real bug: the mutator plants bugs and the score shows how many the tests notice. It judges the tests, it does not write the feature. | `shepherd pattern run mutation-testing --task "Measure how well the tests in src/parser catch bugs"` |
| `planner-workers-reviewer` | You need unit tests for many modules at once: the planner gives one module to each worker and a reviewer checks coverage and quality before anything is reported. Not for a single module. | `shepherd pattern run planner-workers-reviewer --task "Add unit tests for every module under src/services, one worker per module"` |
| `self-debug` | A bug or a failing test survives the first fix and you want attempts that learn from each other: after each failure the solver writes a reflection and the debugger reproduces it independently. | `shepherd pattern run self-debug --task "Fix test_checkout_total: it only fails when a discount code is used"` |

## 2. The workflow

```bash
shepherd pattern new webteam --from orchestrator-worker   # copy a template or an existing pattern
shepherd pattern edit webteam                              # open the YAML in your editor
shepherd pattern validate webteam                          # or: --file path.yaml
shepherd pattern show webteam                              # agents and topics at a glance
shepherd pattern run webteam --task "Objective of the run"
```

```
$ shepherd pattern new webteam --from orchestrator-worker
created pattern 'webteam' at .shepherd/patterns/webteam.yaml
$ shepherd pattern validate webteam
pattern is valid
$ shepherd pattern show webteam
Name:        webteam
Kind:        orchestrator-worker
Source:      project

Agents (2):
  - orchestrator (brain)
  - worker (coder)

Topics (2):
  - orders (4 partitions)
  - results (1 partitions)
```

The terminal UI (Patterns view, `4`) and the native app offer the same steps with a gallery, a graph and an editor: pick a template with `Space`, name it, edit, validate with `v`, and run with `r`.

## 3. The YAML format

```yaml
name: webteam                     # required; letters, digits, hyphen or underscore
kind: orchestrator-worker         # orchestrator-worker | hierarchical | blackboard | market
description: Free text
topics:
- name: orders                    # required and unique
  partitions: 4                   # 1 to 256 (default 1)
  key: task_id                    # informational hint about the partition key
  filter: hypothesis              # optional: prefix expected in messages
  max_messages: 12                # optional: a hard cap; the next publish fails (default: no limit)
agents:
- id: orchestrator                # required and unique
  role: brain                     # a role from your configuration (brings permissions and skills)
  vendor: claude                  # optional: replaces the role's vendor
  model: haiku                    # optional: replaces the role's model
  prompt: Instructions            # the persona; up to 16,384 characters
  count: 1                        # instances (1 to 32; at most 64 agents in total)
  level: 0                        # hierarchical only: tier in the tree
  publishes: [orders]             # topics it writes to
  consumes: [results]             # topics it reads from
  group: worker-pool              # consumer group shared by the instances
  start: true                     # this agent receives the objective and starts the run
```

Things to know:

- `prompt` **replaces** the role's working script. The role contributes only vendor, model, permissions and skills. The agent also receives its topology, its topics and the objective.
- Every `publishes` and `consumes` entry must name a declared topic.
- If no agent has `start: true`, the topology's default starter is used: the orchestrator, the director, the researcher, or the requesters.
- Agents that do not start simply wait. They are woken when a message arrives on a topic they consume.
- With `count > 1` the instances share `group`: each partition is divided among them and **each message goes to exactly one member**.
- Agent ids in a pattern (`worker`) differ from the ids the running agents get (`coder-1`, `coder-2`). Use `shepherd agents` to see the running ones.

## 4. The four kinds of pattern

Every pattern has a `kind` that decides how its agents are wired and which rules the validator checks. There are four kinds, described here. The ten built-in patterns are made from them: the four basic ones have the same name as their kind, and the six for testing are built on `blackboard` and `orchestrator-worker`. [The built-in patterns, one by one](#4b-the-built-in-patterns-one-by-one) says what each of the ten is for.

### orchestrator-worker

One orchestrator splits the objective into orders and aggregates results. A pool of workers does the work.

```
 objective ─► orchestrator ──► [orders] ──► worker ×N ──► [results] ──► orchestrator
```

Rules: exactly **one** orchestrator (`count: 1`, publishes the orders) and **at least one** worker in a consumer group that consumes the orders and publishes results the orchestrator consumes.

To spread orders among workers, the orders topic needs several partitions and the orchestrator must publish **without a key** (or with different keys). With one shared key everything lands on one partition and a single worker gets it all.

```yaml
name: webteam
kind: orchestrator-worker
topics:
- {name: orders, partitions: 4, key: task_id}
- {name: results, partitions: 1}
agents:
- id: orchestrator
  role: brain
  vendor: claude
  model: haiku
  prompt: Split the objective into independent tasks, publish them to orders (without a key) and aggregate the results.
  publishes: [orders]
  consumes: [results]
  start: true
- id: worker
  role: coder
  vendor: claude
  model: haiku
  prompt: When an order arrives, do it in your worktree, commit, and publish the result to results.
  count: 2
  group: worker-pool
  publishes: [results]
  consumes: [orders]
```

### hierarchical

A tree: a director, managers and specialists. Each non-leaf node orchestrates its own subtree: it publishes orders down to the next level and reads results back up.

Rules: every agent needs a `level`; levels are contiguous from 0 with a single root at level 0; topics must not form cycles.

The template has seven agents: `director` (level 0) publishes to `root-orders`; two `manager`s (level 1) read `root-orders`, publish to `team-orders` and report to `root-results`; four `specialist`s (level 2) read `team-orders` and publish to `team-results`.

Use it for large objectives that split naturally into epics, then tasks.

### blackboard

A shared board: several agents read and write the same topic, each contributing its own kind of entry (hypotheses, validations, a synthesis).

Rules: at least two agent instances and at least one topic used (published or read) by **all** of them.

The template has a `researcher` (hypotheses), an `analyst` (validation) and a `synthesizer` (final write-up) on one `blackboard` topic whose `filter` is a message prefix such as `hypothesis:`.

Use it when the problem is exploratory and the steps are not known in advance.

### market

Requesters post asks, solvers bid, and a market-maker decides.

```
 requester ─► [ask] ─► solver ×N ─► [bid] ─► market-maker ─► [transaction] ─► winner works and reports
```

Rules: topics `ask`, `bid` and `transaction` must exist; exactly **one** market-maker that reads `ask` and `bid` and publishes `transaction`; **at least two** solvers that publish to `bid`.

Use it to compete several approaches to the same problem and keep the best.

## 4b. The built-in patterns, one by one

Ten templates come with Shepherd. Four are the basic kinds; six are teams that write and check unit tests, each a known way of making agents give each other feedback. They all validate, run and edit like any pattern you write. In the apps and in `shepherd pattern show <name>` each one explains itself the same way: what it is, when to use it and an example.

How to choose, in one line each:

- **`orchestrator-worker`**: use it when a job splits into independent pieces of similar size: one module, one handler or one file per worker.
- **`hierarchical`**: use it when the objective is big enough to need levels, for example epics that split into tasks.
- **`blackboard`**: use it when the problem is unclear and several viewpoints must read each other's findings: research, hunting a root cause, reviewing a design.
- **`market`**: use it when there are several ways to solve it and you want rival proposals.
- **`reflection-loop`**: use it when one agent's output needs a second pair of eyes before you accept it, new code or new tests, and the standard is clear enough for a critic to judge.
- **`tdd`**: use it when you have a clear specification and want tests that do not mirror the implementation: the test writer never sees the code.
- **`agentcoder`**: use it when you want an independent test suite that includes edge and large cases, and a real test run whose errors go straight back to the programmer.
- **`mutation-testing`**: use it when tests already exist and you doubt they would catch a real bug.
- **`planner-workers-reviewer`**: use it when you need unit tests for many modules at once.
- **`self-debug`**: use it when a bug or a failing test survives the first fix and you want attempts that learn from each other.

Every feedback loop has two caps. The prompts number the rounds (3 or 4): after the last one the agents say what is unresolved and stop. And the shared topic has a `max_messages` limit that the server enforces: the next `publish` fails with a message saying so, the agents stop, and the run shows as `stalled` for a person to decide. The test runs themselves (`tdd`, `agentcoder`) go through the `run_checks` tool, so pass or fail is the exit code of the project's real test command, and the report an agent posts is that tool's output.

### `orchestrator-worker`

One orchestrator splits the objective into orders and a pool of workers does them in parallel.

**Who is in it**

| Agent | Role | What it does |
|---|---|---|
| `orchestrator` | `brain` | Splits the objective, publishes one order per task, aggregates the results. Starts the run. |
| `worker x3` | `coder` | Takes an order, does it in its own worktree, publishes the result. |

**Topics:** `orders` (4 partitions) and `results`. **Kind:** `orchestrator-worker`.

**How it runs**

1. The orchestrator receives the objective and publishes one order per independent task to `orders`.
2. Each order goes to exactly one worker. The worker does it on its own branch and publishes the result to `results`.
3. The orchestrator reads the results and writes the final answer.

**Use it when** a job splits into independent pieces of similar size: one module, one handler or one file per worker. Not when the pieces depend on each other (use `hierarchical`) or when you want rival answers (use `market`).

**Example**

```bash
shepherd pattern run orchestrator-worker --task "Add input validation to the 12 API handlers, one handler per worker"
```

**What to expect**

- Four agents and, usually, three branches to review.
- No message cap by default. Add `max_messages` to a topic if you want one.

### `hierarchical`

A tree: a director, managers and specialists. Each level orders the level below and reads its results.

**Who is in it**

| Agent | Role | What it does |
|---|---|---|
| `director` | `brain` | Level 0. Turns the objective into directives for the managers. Starts the run. |
| `manager x2` | `reviewer` | Level 1. Split a directive into assignments for their specialists and report upward. |
| `specialist x4` | `coder` | Level 2. Do the work and publish to `team-results`. |

**Topics:** `root-orders`, `root-results`, `team-orders`, `team-results`. **Kind:** `hierarchical`.

**How it runs**

1. The director publishes directives to `root-orders`.
2. Each manager reads them, publishes assignments to `team-orders` and waits for `team-results`.
3. The specialists do the work and publish to `team-results`; the managers report to `root-results`; the director reads them.

**Use it when** the objective is big enough to need levels, for example epics that split into tasks. Not for small jobs: the extra layers only cost tokens.

**Example**

```bash
shepherd pattern run hierarchical --task "Move the billing service to the new payments API: backend, API client and tests teams"
```

**What to expect**

- Seven agents run at once, so it is the most expensive of the ten.
- Every agent needs a `level`, contiguous from 0, and topics must not form cycles.

### `blackboard`

A shared board where several agents read each other's findings and add their own, with no fixed order.

**Who is in it**

| Agent | Role | What it does |
|---|---|---|
| `researcher` | `brain` | Turns the objective into a hypothesis and posts it. Starts the run. |
| `analyst` | `tester` | Checks each hypothesis for flaws and edge cases and posts a validation. |
| `synthesizer` | `docs` | Once a hypothesis has a validation, posts a short synthesis of the verified facts. |

**Topics:** one topic, `blackboard`, whose `filter` is the message prefix `hypothesis`. **Kind:** `blackboard`.

**How it runs**

1. The researcher posts `hypothesis: ...`.
2. The analyst answers with `validation: ...`.
3. The synthesizer posts `synthesis: ...` once a hypothesis is validated.

**Use it when** the problem is unclear and several viewpoints must read each other's findings: research, hunting a root cause, reviewing a design. Not when the work splits cleanly into parts.

**Example**

```bash
shepherd pattern run blackboard --task "Find out why the nightly import takes four hours"
```

**What to expect**

- Three agents, no worktree needed unless a role asks for one.
- There is no natural end: it is `done` when everyone is idle and nothing new was posted.

### `market`

Requesters post asks, solvers bid, a market-maker picks the winner and that solver does the work.

**Who is in it**

| Agent | Role | What it does |
|---|---|---|
| `requester x2` | `docs` | Post the ask from the objective. Start the run. |
| `solver x3` | `coder` | Bid on an ask; the winner implements it. |
| `market-maker` | `brain` | Reads asks and bids and publishes the transaction. |

**Topics:** `ask` (2 partitions), `bid` (2) and `transaction`. **Kind:** `market`.

**How it runs**

1. The requesters publish an `ask` built from the objective.
2. The solvers publish a `bid` each.
3. The market-maker reads asks and bids and publishes the `transaction`; the winner works and reports.

**Use it when** there are several ways to solve it and you want rival proposals. It costs more than one agent, so use it when the choice matters.

**Example**

```bash
shepherd pattern run market --task "Choose and implement the fastest way to deduplicate 10 million rows"
```

**What to expect**

- Six agents. Needs at least two solvers publishing to `bid`.
- Only the winning solver's branch is worth merging.

### `reflection-loop`

A generator writes code or tests; a critic from another vendor reviews them and sends comments back until it approves.

**Who is in it**

| Agent | Role | What it does |
|---|---|---|
| `generator` | `coder` | Writes the draft in its own worktree and posts `draft: round N/3`. Starts the run. |
| `critic` | `reviewer` | Reads the diff and posts `critique: round N/3`: APPROVE, or REQUEST_CHANGES with `file:line` comments. |

**Topics:** one topic, `board`, capped at 10 messages. **Kind:** `blackboard`.

**How it runs**

1. The generator posts `draft: round 1/3` with its branch.
2. The critic reads the diff and posts APPROVE or REQUEST_CHANGES.
3. On REQUEST_CHANGES the generator fixes every comment and posts the next draft. It stops when the critic approves.

**Use it when** one agent's output needs a second pair of eyes before you accept it, new code or new tests, and the standard is clear enough for a critic to judge. Not for open exploration.

**Example**

```bash
shepherd pattern run reflection-loop --task "Write unit tests for src/billing/invoice.rs; the critic must approve before I read them"
```

**What to expect**

- At most 3 rounds (the prompts) and 10 messages (the server). If they hit the cap the run shows as `stalled` and you decide.
- The critic never edits files.

### `tdd`

Tests first. A test writer works from the specification alone, a coder implements until they pass and a runner runs the real tests.

**Who is in it**

| Agent | Role | What it does |
|---|---|---|
| `test-writer` | `tester` | Writes the failing tests from the spec without reading any implementation. Starts the run. |
| `coder` | `coder` | Checks out the tests and implements until they pass. Never edits the tests. |
| `runner` | `tester` | Calls `run_checks` on the coder's branch and posts the exact report. |

**Topics:** one topic, `board`, capped at 16 messages. **Kind:** `blackboard`.

**How it runs**

1. The test writer posts `tests:` with its branch and the command that runs them.
2. The coder posts `code: round N/4`.
3. The runner posts `failures: round N/4` with the real output, or `passed`. The coder fixes the code, not the tests.

**Use it when** you have a clear specification and want tests that do not mirror the implementation: the test writer never sees the code. Not when the spec is still vague.

**Example**

```bash
shepherd pattern run tdd --task "Implement slugify(title) as described in docs/slugify.md: lowercase, hyphens, no accents"
```

**What to expect**

- At most 4 rounds and 16 messages.
- Pass or fail is the exit code of your checks, copied as is; it is only as good as your `workflow.checks`.

### `agentcoder`

A programmer, an independent test designer and an executor, after the AgentCoder paper.

**Who is in it**

| Agent | Role | What it does |
|---|---|---|
| `programmer` | `coder` | Implements and posts `code: round N/4`. Starts the run. |
| `test-designer` | `tester` | Writes one suite from the spec alone: basic, edge and large cases. |
| `executor` | `tester` | Runs the suite against the code with `run_checks` and posts the real errors. |

**Topics:** one topic, `board`, capped at 16 messages. **Kind:** `blackboard`.

**How it runs**

1. The programmer posts `code:` and the test designer posts `tests:`, independently.
2. The executor puts them together, calls `run_checks` and posts `errors: round N/4` or `passed`.
3. The programmer fixes the code using those real errors.

**Use it when** you want an independent test suite that includes edge and large cases, and a real test run whose errors go straight back to the programmer. Needs a project whose tests run from the command line.

**Example**

```bash
shepherd pattern run agentcoder --task "Implement an LRU cache with a capacity and a time to live"
```

**What to expect**

- At most 4 rounds and 16 messages.
- If your project has no checks to run, the executor says so and stops.

### `mutation-testing`

Measures whether tests are worth anything: a mutator plants bugs, a test writer must catch them, a judge scores the result.

**Who is in it**

| Agent | Role | What it does |
|---|---|---|
| `mutator` | `coder` | Creates up to 8 mutants, each a small deliberate bug on its own branch. Never touches the original. Starts the run. |
| `test-writer` | `tester` | Adds tests that must fail on a surviving mutant and pass on the original. |
| `judge` | `reviewer` | Runs the tests on the original and on every mutant and posts the score and the survivors. |

**Topics:** one topic, `board`, capped at 16 messages. **Kind:** `blackboard`.

**How it runs**

1. The mutator posts `mutants:` with one line per mutant.
2. The judge posts `score: round N/3` (killed / total) and `survivors:`.
3. The test writer adds tests aimed at each survivor; the loop repeats until every mutant is killed or round 3.

**Use it when** tests already exist and you doubt they would catch a real bug. It judges the tests; it does not write the feature.

**Example**

```bash
shepherd pattern run mutation-testing --task "Measure how well the tests in src/parser catch bugs"
```

**What to expect**

- At most 3 rounds and 16 messages.
- A mutant must still compile, so a surviving one is a real gap in the tests.

### `planner-workers-reviewer`

A planner gives one module to each worker, each worker writes that module's tests, and a reviewer checks coverage and quality before anything is reported.

**Who is in it**

| Agent | Role | What it does |
|---|---|---|
| `planner` | `brain` | Splits the objective into modules, orders one per module, reports only what the reviewer approved. Starts the run. |
| `worker x3` | `coder` | Writes the unit tests of its module and publishes the result. |
| `reviewer` | `reviewer` | Compares each result with its order, checks coverage and quality, publishes APPROVE or REQUEST_CHANGES. |

**Topics:** `orders` (4 partitions), `results` and `reviews`. **Kind:** `orchestrator-worker`.

**How it runs**

1. The planner publishes one order per module to `orders`.
2. A worker writes the tests, runs them, commits and publishes to `results`.
3. The reviewer publishes to `reviews`; the planner collects the approved ones. A module rejected twice is reported as not done.

**Use it when** you need unit tests for many modules at once. Not for a single module.

**Example**

```bash
shepherd pattern run planner-workers-reviewer --task "Add unit tests for every module under src/services, one worker per module"
```

**What to expect**

- Five agents and three branches. Nothing is merged by the run.
- Workers get at most one retry.

### `self-debug`

Reflexion and self-debug: after each failing run the solver writes a reflection, and the board keeps them as memory.

**Who is in it**

| Agent | Role | What it does |
|---|---|---|
| `solver` | `coder` | Implements, runs the tests, and after a failure posts `reflection:` (what failed, why, what it will change). Starts the run. |
| `debugger` | `tester` | Reproduces the failure on its own and posts `diagnosis:` with the root cause and the smallest fix. |

**Topics:** one topic, `board`, capped at 16 messages. **Kind:** `blackboard`.

**How it runs**

1. The solver posts `attempt: N/4`.
2. On failure it posts a `reflection:` and the debugger posts a `diagnosis:`.
3. Each next attempt starts by reading every earlier reflection and diagnosis, and does not repeat a change that already failed.

**Use it when** a bug or a failing test survives the first fix and you want attempts that learn from each other.

**Example**

```bash
shepherd pattern run self-debug --task "Fix test_checkout_total: it only fails when a discount code is used"
```

**What to expect**

- At most 4 attempts and 16 messages, then it posts `blocked:` with what is unresolved.
- The board is the memory, so the whole run's reflections stay readable in the Events view.

## 5. Validation

`pattern validate` reports each problem with a severity and a field path:

```
$ shepherd pattern validate t-market
pattern has validation problems:
  [warning] agents[0].vendor: vendor 'claude' is configured but its command 'claude' is not installed or not in PATH
  [error] agents: market pattern requires at least 2 solver agent instances publishing to 'bid', found 1
```

An `error` stops a run. A `warning` informs you (for example, a vendor that is not installed, a model the vendor does not list, or a topic nobody publishes to or reads from).

What is checked:

- **Identifiers and limits:** valid, unique pattern name and agent ids; at least one agent and at most 64; `count` from 1 to 32; `partitions` from 1 to 256; persona up to 16,384 characters.
- **Topics:** every topic named in `publishes` or `consumes` exists; duplicate topic names are an error.
- **Configuration:** the `role` exists in your configuration; the vendor and model are known.
- **Topology:** the rules of the chosen `kind`, listed above.

The editors in the terminal UI and the native app show the same messages next to the field that caused them.

## 6. Running and controlling a run

```bash
shepherd pattern run webteam --task "..." [--worktree /path] [--confirm-over-limit]
shepherd pattern runs                  # ID PATTERN KIND STATE AGENTS LIVE TOPICS CREATED
shepherd pattern pause <id>
shepherd pattern resume <id>
shepherd pattern stop <id> --confirm
shepherd pattern delete-run <id> [--confirm]   # an active run is stopped first, which needs --confirm
shepherd pattern clean                         # forget the runs that are done, failed or stopped
```

In the terminal UI, Events > Pattern Runs, `D` deletes the selected run and `C` forgets the finished ones; both ask first.

```
$ shepherd pattern run duo --task "Task #1: demo"
started pattern run run-duo-11624 for pattern duo (running) with 2 agents
$ shepherd pattern runs
ID             PATTERN  KIND        STATE    AGENTS  LIVE  TOPICS  CREATED
run-duo-11624  duo      blackboard  running  2       2     1       2026-11-09T16:38:51Z
```

What a run does:

- Starts one agent per instance with its role, vendor, model, persona, skills and topics. Roles with `worktree: true` get their own branch and worktree (`shep/<n>-run-<pattern>-...`), or all agents share one directory if you pass `--worktree`.
- Gives starters the objective as their first order and tells the others to wait for messages.
- If the run would exceed `max_concurrent` active agents, it asks for `--confirm-over-limit`.
- Never merges or pushes. Each agent leaves commits on its branch and you integrate with git.

To link a run to a task, start the objective with `Task #<id>:`. The Tasks view then shows the run and closes the task when the run finishes.

### Run states

| State | Meaning |
|---|---|
| `running` | Agents are working or waiting for messages |
| `paused` | Paused with `pattern pause` |
| `done` | Every agent is idle and nothing was published for `run_quiescence_secs` (default 30). A new message reopens the run |
| `degraded` | Some agent ended or died before the run completed |
| `failed` | Every agent failed or died |
| `stalled` | Agents are idle or blocked and the run did not finish well: an agent is blocked, or nothing was published. The reason is shown with the run |
| `stopped` | Stopped with `pattern stop` |

An agent waiting for a human answer counts as idle, so answer its questions or give it an objective that needs no approval.

## 7. Designing a good pattern

- **Start with a template** and change as little as possible. The templates already pass validation.
- **Give each agent a sharp persona.** The `prompt` is the whole job description, since it replaces the role's script. Say what to read, what to do, and what to publish where.
- **Match partitions to workers.** For N workers use a topic with at least N partitions and publish without a key.
- **Keep worktree roles few.** Each `worktree: true` agent costs a branch you will review and merge.
- **Mix vendors on purpose.** Use `vendor` and `model` per agent: a cheap model for workers, a stronger one for the orchestrator.
- **Validate before running.** A warning about a missing vendor is much cheaper to read than a run that cannot start.
- **Watch the bus.** `shepherd topic tail <topic>` shows what agents actually say to each other. See [topics-and-events.md](topics-and-events.md).

## 8. End to end example

An orchestrator and two workers add two independent features to a Go service.

```bash
cd ~/webgo

# 1) copy, adjust and validate
shepherd pattern new webteam --from orchestrator-worker
#    edit .shepherd/patterns/webteam.yaml: vendor claude, model haiku, worker count 2
shepherd pattern validate webteam

# 2) run it with an objective
shepherd pattern run webteam --task \
  "One worker adds GET /version (version.go + test); the other adds a logging middleware (logging.go + test). Each in its own worktree, not touching the other's files, and commit."

# 3) watch
shepherd pattern runs
shepherd agents
shepherd topic groups orders        # worker-pool with 2 members and the lag per partition
shepherd topic tail results         # the workers' reports

# 4) when the run is done, integrate with plain git
git worktree list
git merge --no-edit shep/6-run-webteam-189105
git merge --no-edit shep/7-run-webteam-189105
go vet ./... && go test ./...

# 5) clean up
git worktree remove ~/.shepherd/worktrees/webgo/shep-6-run-webteam-189105
git branch -d shep/6-run-webteam-189105 shep/7-run-webteam-189105
```

Each worker ends with its own branch, commit and snapshots. The two branches merge cleanly because the workers did not touch each other's files.
