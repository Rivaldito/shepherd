//! `shepherd <group> <command> --help`: the usage and a one-line description of that command only, for the
//! groups that have several (task, pattern, topic, snapshot, skills). The long usage that lists every command
//! stays what `shepherd --help` prints.

struct Group {
    name: &'static str,
    about: &'static str,
    commands: &'static [Command],
}

struct Command {
    name: &'static str,
    usage: &'static str,
    about: &'static str,
}

const fn cmd(name: &'static str, usage: &'static str, about: &'static str) -> Command {
    Command { name, usage, about }
}

const GROUPS: &[Group] = &[
    Group {
        name: "task",
        about: "Create, read, edit, archive and delete tasks, and import them from GitHub.",
        commands: &[
            cmd("new", "<title> [--description <text>] [--criteria <text>]... [--id <number>]", "Create a task in .shepherd/tasks/."),
            cmd("list", "[--archived]", "List the tasks; --archived lists the archived ones."),
            cmd("show", "<id> [section]", "Print a task, or one section of it."),
            cmd("plan", "<id> [--save]", "Have the planner split a task into subtasks; --save stores the plan."),
            cmd("edit", "<id> [section]", "Open a task, or one section of it, in your editor."),
            cmd("path", "<id>", "Print the folder of a task."),
            cmd("archive", "<id>", "Hide a task; its folder is kept."),
            cmd("delete", "<id> [--archived] --confirm", "Remove a task's folder for good; it refuses without --confirm."),
            cmd("import", "<issue-number-or-url>", "Create a task from a GitHub issue (needs gh)."),
        ],
    },
    Group {
        name: "pattern",
        about: "Teams of agents described in YAML: manage them and run them.",
        commands: &[
            cmd("list", "", "List the built-in patterns and your own."),
            cmd("show", "<name>", "Show a pattern's agents and topics."),
            cmd("validate", "<name|--file path>", "Check a pattern before running it."),
            cmd("delete", "<name>", "Delete one of your patterns."),
            cmd("edit", "<name>", "Open a pattern's YAML in your editor."),
            cmd("new", "<name> --from <template|name> [--global]", "Copy a template or an existing pattern under a new name."),
            cmd("run", "<name> [--task <task>] [--worktree <path>] [--confirm-over-limit]", "Start a run of a pattern. This starts real agents."),
            cmd("runs", "", "List the pattern runs and their state."),
            cmd("pause", "<id>", "Pause a run."),
            cmd("resume", "<id>", "Resume a paused run."),
            cmd("stop", "<id> [--confirm]", "Stop a run and close its agents; it needs --confirm."),
            cmd("clean", "", "Forget the runs that are done, failed or stopped."),
            cmd("delete-run", "<id> [--confirm]", "Remove one run; an active one is stopped first, which needs --confirm."),
        ],
    },
    Group {
        name: "topic",
        about: "The event bus the agents talk through.",
        commands: &[
            cmd("list", "", "List the topics with their partitions and message counts."),
            cmd("create", "<name> [--partitions <n>]", "Create a topic."),
            cmd("groups", "[topic]", "Show the consumer groups and how far behind they are."),
            cmd("publish", "<topic> <content> [--key <key>] [--partition <p>] [--from <from>]", "Publish a message to a topic."),
            cmd("tail", "<topic> [--partition <p>] [--limit <n>]", "Show the latest messages of a topic."),
            cmd("replay", "<topic> [--partition <p>] [--from <offset>] [--limit <n>]", "Read the messages of a topic from an offset."),
        ],
    },
    Group {
        name: "snapshot",
        about: "Local copies of an agent's worktree. Ids look like <agent>/<n>; add --no-server to run without a server.",
        commands: &[
            cmd("capture", "<agent> [--reason manual|idle|done|pre-restore|pre-clean|pre-kill] [--json]", "Take a snapshot of an agent's worktree now."),
            cmd("list", "[--agent <agent>] [--all] [--json]", "List the snapshots."),
            cmd("show", "<id> [--full] [--json]", "Show what a snapshot changed."),
            cmd("diff", "<id> [<id>] [--full] [--json]", "Compare a snapshot with the previous one, with another, or with the worktree now."),
            cmd("restore", "<id> [--path <file>]... [--confirm] [--delete-extra] [--json]", "Put a snapshot back; it saves a pre-restore snapshot first."),
            cmd("delete", "<id> [--json]", "Delete a snapshot."),
            cmd("prune", "[--agent <agent>] [--keep <n>] [--json]", "Keep the newest snapshots and delete the rest."),
        ],
    },
    Group {
        name: "skills",
        about: "The skills agents are given.",
        commands: &[
            cmd("list", "", "List the skills of the catalog."),
            cmd("create", "<name> [--description <text>] [--repo] [--global]", "Create a skill, for you or (with --repo) for the project."),
            cmd("delete", "<name>", "Delete a skill; a built-in one cannot be deleted."),
            cmd("global", "<name> [--off]", "Give a skill to every agent, or stop doing so with --off."),
            cmd("agent", "<agent> <skill> on|off|default", "Change one agent's skills; it applies the next time the agent starts."),
        ],
    },
];

fn usage_line(group: &Group, command: &Command) -> String {
    if command.usage.is_empty() {
        format!("usage: shepherd {} {}", group.name, command.name)
    } else {
        format!(
            "usage: shepherd {} {} {}",
            group.name, command.name, command.usage
        )
    }
}

/// The help for `args` when they ask for it (`--help` or `-h`) inside one of the groups: the command's usage and
/// description, or the group's list of commands when no command is named. `None` for everything else, which keeps
/// the general usage.
pub(crate) fn help_text(args: &[String]) -> Option<String> {
    if !args.iter().any(|a| a == "--help" || a == "-h") {
        return None;
    }
    let mut words = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .map(String::as_str);
    let first = words.next();
    let group = GROUPS.iter().find(|g| Some(g.name) == first)?;
    if let Some(command) = words
        .next()
        .and_then(|w| group.commands.iter().find(|c| c.name == w))
    {
        return Some(format!(
            "{}\n\n{}",
            usage_line(group, command),
            command.about
        ));
    }
    let mut text = format!(
        "usage: shepherd {} <command> [options]\n\n{}\n\ncommands:\n",
        group.name, group.about
    );
    let width = group
        .commands
        .iter()
        .map(|c| c.name.len())
        .max()
        .unwrap_or(0);
    for c in group.commands {
        text.push_str(&format!("  {:width$}  {}\n", c.name, c.about));
    }
    text.push_str(&format!(
        "\nshepherd {} <command> --help shows one command's options.",
        group.name
    ));
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn help(line: &str) -> Option<String> {
        help_text(
            &line
                .split_whitespace()
                .map(String::from)
                .collect::<Vec<_>>(),
        )
    }

    /// What the help of every command of a group must be: its own usage and description, and nothing of the
    /// other commands or of the long general usage.
    fn check_group(name: &str) {
        let group = GROUPS.iter().find(|g| g.name == name).unwrap();
        // the group lists every command, with its description
        let listing = help(&format!("{name} --help")).unwrap();
        assert!(
            listing.starts_with(&format!("usage: shepherd {name} <command>")),
            "{listing}"
        );
        for c in group.commands {
            assert!(
                listing.contains(c.about),
                "{name} {} is missing from the group help",
                c.name
            );
        }
        for c in group.commands {
            for flag in ["--help", "-h"] {
                let text = help(&format!("{name} {} {flag}", c.name)).unwrap();
                assert_eq!(
                    text,
                    format!("{}\n\n{}", usage_line(group, c), c.about),
                    "{name} {}",
                    c.name
                );
                assert_eq!(text.matches("usage:").count(), 1, "{text}");
                for other in group.commands.iter().filter(|o| o.name != c.name) {
                    assert!(
                        !text.contains(other.about),
                        "{name} {} shows {}",
                        c.name,
                        other.name
                    );
                }
                // not the long usage of every command
                assert!(
                    !text.contains("shepherd mcp serve") && !text.contains("agents\n"),
                    "{text}"
                );
                assert!(
                    text.lines().count() <= 3,
                    "one command's help is short: {text}"
                );
            }
        }
        // a flag in the middle does not change the answer
        let first = &group.commands[0];
        assert_eq!(
            help(&format!("{name} --json {} --help", first.name)),
            help(&format!("{name} {} --help", first.name))
        );
    }

    #[test]
    fn task_help_is_per_command() {
        check_group("task");
        let text = help("task delete --help").unwrap();
        assert!(
            text.contains("--confirm") && text.contains("<id> [--archived]"),
            "{text}"
        );
    }

    #[test]
    fn pattern_help_is_per_command() {
        check_group("pattern");
        assert!(help("pattern run --help")
            .unwrap()
            .contains("--confirm-over-limit"));
    }

    #[test]
    fn topic_help_is_per_command() {
        check_group("topic");
        let text = help("topic publish --help").unwrap();
        assert!(
            text.starts_with("usage: shepherd topic publish <topic> <content> [--key <key>]"),
            "{text}"
        );
        assert!(text.ends_with("Publish a message to a topic."), "{text}");
    }

    #[test]
    fn snapshot_help_is_per_command() {
        check_group("snapshot");
        assert!(help("snapshot restore --help")
            .unwrap()
            .contains("--delete-extra"));
    }

    #[test]
    fn skills_help_is_per_command() {
        check_group("skills");
        assert!(help("skills agent --help")
            .unwrap()
            .contains("on|off|default"));
    }

    #[test]
    fn everything_else_keeps_the_general_usage() {
        for line in [
            "--help",
            "agents --help",
            "git branch --help",
            "topic nope --nothing",
            "topic list",
            "nope --help",
        ] {
            assert_eq!(help(line), None, "{line}");
        }
        // an unknown command in a group gets the group's list, not the general usage
        assert!(help("topic nope --help").unwrap().contains("commands:"));
    }

    #[test]
    fn every_command_is_in_the_general_usage_too() {
        // keeps the two descriptions of the commands from drifting apart
        for g in GROUPS {
            for c in g.commands {
                assert!(
                    super::super::shepherd_bindings::USAGE_STRING.contains(c.name),
                    "{} {} is not in `shepherd --help`",
                    g.name,
                    c.name
                );
            }
        }
    }
}
