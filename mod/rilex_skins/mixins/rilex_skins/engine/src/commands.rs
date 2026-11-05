// RILEX SKINS — © 2026 RILEX MODS (rilexDEV). MIT licence: keep this notice in every copy.
// /rilexskin ... — parsed here, standard library only.
// Not /skin: servers with SkinsRestorer already use /skin, and the mod would swallow it.

use crate::proto::Model;

pub const ROOT: &str = "rilexskin";

#[derive(Debug, PartialEq, Eq)]
pub enum Cmd {
    Help,
    Status,
    Register(String),
    Set { path: String, model: Model },
    Recover { name: String, code: String },
    Finish,
    Backup,
    Restore(String),
    Api(String),
    Bad(String),
}

/// `command` is what the player typed, without the slash. None: not ours, let it through.
pub fn parse(command: &str) -> Option<Cmd> {
    let rest = command.strip_prefix(ROOT)?;
    if !(rest.is_empty() || rest.starts_with(' ')) {
        return None; // "/rilexskinx" is someone else's
    }
    let rest = rest.trim();
    let (sub, args) = match rest.split_once(' ') {
        Some((s, a)) => (s, a.trim()),
        None => (rest, ""),
    };
    Some(match sub.to_ascii_lowercase().as_str() {
        "" | "help" => Cmd::Help,
        "status" => Cmd::Status,
        "register" if !args.is_empty() && !args.contains(' ') => Cmd::Register(args.to_string()),
        "register" => Cmd::Bad("usage: /rilexskin register <your name>".into()),
        "set" if !args.is_empty() => {
            // The path may hold spaces; an optional last word picks the arm model.
            let (path, model) = match args.rsplit_once(' ') {
                Some((p, m)) if Model::parse(m).is_some() => (p.trim(), Model::parse(m).unwrap()),
                _ => (args, Model::Classic),
            };
            Cmd::Set { path: unquote(path), model }
        }
        "set" => Cmd::Bad("usage: /rilexskin set <path to skin.png> [classic|slim]".into()),
        "recover" => match args.split_once(' ') {
            Some((name, code)) if !code.trim().is_empty() => Cmd::Recover { name: name.to_string(), code: code.trim().to_string() },
            _ => Cmd::Bad("usage: /rilexskin recover <name> <recovery code>".into()),
        },
        "finish" => Cmd::Finish,
        "backup" => Cmd::Backup,
        "restore" if !args.is_empty() => Cmd::Restore(unquote(args)),
        "restore" => Cmd::Bad("usage: /rilexskin restore <path to .rilexkey file>".into()),
        "api" if args.starts_with("https://") => Cmd::Api(args.to_string()),
        "api" => Cmd::Bad("usage: /rilexskin api https://your-worker.workers.dev".into()),
        other => Cmd::Bad(format!("unknown: {other}. Try /rilexskin help")),
    })
}

fn unquote(s: &str) -> String {
    s.trim().trim_matches('"').to_string()
}

pub const HELP: &[&str] = &[
    "[RILEX SKINS] by RILEX MODS - commands:",
    "/rilexskin register <name> - claim your name (once)",
    "/rilexskin set <path to skin.png> [classic|slim] - upload your skin",
    "/rilexskin status - what this computer holds",
    "/rilexskin backup - save your key to a file (keep it safe!)",
    "/rilexskin restore <file> - use your key on a new computer",
    "/rilexskin recover <name> <code> - lost the key? use your recovery code",
    "/rilexskin finish - after the admin approved your recovery",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        assert_eq!(parse("say hi"), None);
        assert_eq!(parse("rilexskinx"), None);
        assert_eq!(parse("rilexskin"), Some(Cmd::Help));
        assert_eq!(parse("rilexskin register Rilex"), Some(Cmd::Register("Rilex".into())));
        assert!(matches!(parse("rilexskin register two words"), Some(Cmd::Bad(_))));
        assert_eq!(
            parse(r#"rilexskin set "C:\My Skins\me.png" slim"#),
            Some(Cmd::Set { path: r"C:\My Skins\me.png".into(), model: Model::Slim })
        );
        assert_eq!(
            parse("rilexskin set /home/a/me.png"),
            Some(Cmd::Set { path: "/home/a/me.png".into(), model: Model::Classic })
        );
        assert_eq!(
            parse("rilexskin recover Rilex RLX-AAAA-BBBB-CCCC-DDDD"),
            Some(Cmd::Recover { name: "Rilex".into(), code: "RLX-AAAA-BBBB-CCCC-DDDD".into() })
        );
        assert!(matches!(parse("rilexskin api http://insecure"), Some(Cmd::Bad(_))));
    }
}
