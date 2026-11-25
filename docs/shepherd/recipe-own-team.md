# Your own role, skill and pattern: a Go server

Teach Shepherd your conventions with a skill, define a specialist with a role, build a team from a built-in pattern, and let it write a Go HTTP server that you review and merge yourself.

This walks through the three things you create yourself: a **skill** (how your project does things), a **role** (who does the job) and a **pattern** (who talks to whom). The team writes a small Go server; a second agent runs the real checks before approving; you merge the branch. About 10 minutes, most of it waiting.

Needs `git`, Go 1.22 or newer, Shepherd 0.5.3 or newer and the `claude` CLI signed in. The recipe uses claude's `haiku` model, the cheapest, for both agents.

> Everything below was run with the CLI against a real claude haiku team and produced a working server. The desktop app and the terminal UI offer the same steps; where they differ, the CLI command is given.

## 1. Start from an empty project

```
mkdir gohello && cd gohello
git init -b main
go mod init example.com/gohello
printf '# gohello\n' > README.md
printf '.shepherd/pattern_runs.json\n' > .gitignore
git add -A && git commit -m "chore: start the project"
shepherd                      # opens the app (shepherd --tui for the terminal)
```

The `.gitignore` line keeps the run log out of git, so the Git view shows `main` as clean after a run.

## 2. Write the skill

A skill is a short manual the agent loads when its one-line description fits the work. Create it in the project, so it travels with the code:

```
shepherd skills create go-http-conventions --repo \
  --description "Use when writing Go HTTP handlers or servers - standard library net/http, JSON errors, graceful shutdown and httptest tests." 
```

Then edit `.shepherd/skills/go-http-conventions/SKILL.md` (or use the Skills view, which has an editor) and write the rules:

```
---
name: go-http-conventions
description: Use when writing Go HTTP handlers or servers - standard library net/http, JSON errors, graceful shutdown and httptest tests.
---

# go-http-conventions

1. Use only the standard library: `net/http` with the Go 1.22 `ServeMux` patterns (`mux.HandleFunc("GET /health", ...)`). Add no web framework.
2. Put handlers in their own file and build the server in `NewServer(addr string) *http.Server`, so tests can use it without opening a port.
3. Answer every error as JSON, `{"error": "<message>"}`, with the right status code and `Content-Type: application/json`.
4. `main` reads the address from the `ADDR` environment variable (default `:8080`) and shuts down gracefully on SIGINT and SIGTERM with `Server.Shutdown` and a timeout.
5. Test every handler with `net/http/httptest`: the happy path, a wrong method, and bad input.
6. Before delivering, `gofmt -l .` prints nothing, `go vet ./...` and `go test ./...` pass.
```

## 3. Add the role

A role says who does a job: vendor, model, whether it gets its own worktree, and which skills it carries. Add a `gopher` to `.shepherd/config.yaml`, together with the checks that decide whether Go code is good:

```
roles:
  gopher:                    # a new role: Go server specialist
    vendor: claude
    model: haiku
    worktree: true
    prompt: You are a Go engineer. You write small, idiomatic, tested HTTP servers with the standard library only.
    skills: [lang-go, go-http-conventions]
workflow:
  checks:
    - gofmt -l . | (! grep .)
    - go vet ./...
    - go test ./...
```

```
shepherd config validate      # configuration is valid
shepherd roles list           # gopher  claude/haiku
```

In the app, **Agents > New role** does the same without editing YAML. `workflow.checks` are what the `run_checks` tool runs; without them Shepherd detects `go test ./...`, `cargo test`, `pytest` or `npm test` from the files of the project.

## 4. Build the pattern from a built-in one

Do not start from a blank file. Open the **Patterns** view and read the built-in `reflection-loop`: its card says what it is for and gives an example. It is a generator and a critic that correct each other until the critic approves, which is what we want. Copy it:

```
shepherd pattern new go-server --from reflection-loop
```

Then change three things in `.shepherd/patterns/go-server.yaml` (in the app: **Duplicate**, then the graph editor): the generator becomes your `gopher`, the critic runs on claude haiku, and the critic is told to run the real checks first.

```
name: go-server
kind: blackboard
description: A Go engineer (the gopher role, with the go-http-conventions skill) writes a small HTTP server in its own worktree; a reviewer runs the real checks and reviews it until it approves. Capped at 3 rounds and 10 messages.
topics:
- name: board
  partitions: 1
  max_messages: 10          # a hard cap: the 11th publish is refused
agents:
- id: generator
  role: gopher              # your new role
  prompt: 'You are the generator. Build what the run objective asks for in your worktree, following your go-http-conventions skill, and commit it. Post ''draft: round 1/3'' on the board with your branch and a short summary. When a ''critique:'' arrives, fix every comment, commit, and post the next draft (''draft: round N/3''). Stop when the critic approves. Never go past round 3: if the critic still asks for changes after round 3, post ''blocked: '' plus what is unresolved and stop.'
  publishes: [board]
  consumes: [board]
  start: true
- id: critic
  role: reviewer
  vendor: claude            # override the role's vendor and model for this agent
  model: haiku
  prompt: 'You are the critic. When a ''draft:'' arrives, call the run_checks tool with the generator''s agent id: it runs the project''s real checks (gofmt, go vet, go test). Then read the generator''s diff and review correctness, error handling, tests and the Go conventions. Post ''critique: round N/3'' followed by APPROVE or REQUEST_CHANGES. Never approve while run_checks says FAILED: paste its output as the first comment. Approve only when the checks pass and no blocker or major comment is left. Do not edit files. Do not ask for a fourth round.'
  publishes: [board]
  consumes: [board]
```

```
shepherd pattern validate go-server    # pattern is valid
```

> Why claude for the critic? With the default `reviewer` role (gemini, through agy) the loop took about ten minutes in testing and asked for permission to run about eight commands, because that agent did not use `run_checks`. With the critic on claude haiku the whole loop took under a minute and the critic called `run_checks`. Codex was not tried.

## 5. Run it

```
shepherd pattern run go-server --task 'Build a small HTTP server in Go: GET /health returns {"status":"ok"}; GET /hello?name=Ana returns {"message":"Hello, Ana!"} (name defaults to World). Include tests.'
shepherd agents                              # gopher-1 and reviewer-1 go from starting to working
shepherd topic replay board --from 0         # the messages, as they were published
```

Two things to expect:

- **A trust prompt.** The first time claude opens a folder it asks whether you trust it, and the agent shows as `blocked` until a person answers. Shepherd never answers permission prompts for you. Open the agent's terminal (`shepherd attach gopher-1`) and answer.
- **The conversation.** The generator posts `draft: round 1/3` with its branch. The critic calls `run_checks`, pastes its output, and posts `critique: round 1/3` with APPROVE or REQUEST_CHANGES. In testing the draft arrived after about 45 seconds and the critic approved in round 1, with `PASSED` on `gofmt`, `go vet` and `go test`.

## 6. Review and merge in the Git view

```
shepherd git status
```

```
BRANCH                      STATUS  SYNC    AGENT     PR  PATH
main                        clean   up to date  -     -   /path/to/gohello
shep/1-run-go-server-9238   clean   +1/-0   gopher-1  -   ~/.shepherd/worktrees/gohello/shep-1-run-go-server-9238
```

The **Git** view shows the same table. `main` has not moved: the agent worked on its own branch, one commit ahead (`+1/-0`), in its own folder. Read what it did and merge it yourself; Shepherd never merges for you:

```
git diff --stat main..shep/1-run-go-server-9238
git merge --no-edit shep/1-run-go-server-9238
go test ./...
shepherd git worktree remove ~/.shepherd/worktrees/gohello/shep-1-run-go-server-9238
```

Run the result:

```
ADDR=:8080 go run . &
curl localhost:8080/health                 # {"status":"ok"}
curl 'localhost:8080/hello?name=Ana'       # {"message":"Hello, Ana!"}
curl localhost:8080/hello                  # {"message":"Hello, World!"}
curl -i -X POST localhost:8080/health      # 405 with {"error":"method not allowed"}
```

## What each piece did

- **The skill** gave the generator your rules (standard library, `NewServer`, JSON errors, graceful shutdown, `httptest`) without repeating them in every prompt. It is in the repo, so a colleague gets it with `git pull`.
- **The role** said who fills the job: claude haiku, in its own worktree, carrying that skill.
- **The pattern** said who talks to whom: a generator and a critic on one topic, capped at 3 rounds and 10 messages.
- **The check** made the review about facts: `run_checks` ran your `workflow.checks` and the critic could not approve while it said FAILED.
- **The worktree** kept your `main` untouched until you merged.

## Variations

- Start from `tdd` instead of `reflection-loop` to have the tests written first, from the specification alone: `shepherd pattern new go-server --from tdd`, then point the `coder` agent at `gopher`.
- Keep the same role and skill and change the objective: they are reusable, only the `--task` changes.
- Add `max_messages` to any topic of your own patterns to cap a loop for good.
- Share it: commit `.shepherd/config.yaml`, `.shepherd/skills/` and `.shepherd/patterns/`. Everyone who clones the repo has the same role, skill and team.
