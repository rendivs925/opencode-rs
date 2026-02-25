use napi::Result;

#[napi(object)]
#[derive(Clone)]
pub struct BashCommand {
    pub text: String,
    pub command: Vec<String>,
}

#[napi(object)]
#[derive(Clone)]
pub struct BashParseResult {
    pub commands: Vec<BashCommand>,
}

#[napi]
pub fn parse_bash_command(input: String) -> Result<BashParseResult> {
    let commands = split_commands(&input)
        .into_iter()
        .map(|text| {
            let tokens = split_words(&text);
            let mut start = 0usize;
            while start < tokens.len() && is_assignment(&tokens[start]) {
                start += 1;
            }
            BashCommand {
                text,
                command: tokens[start..].to_vec(),
            }
        })
        .collect::<Vec<_>>();
    Ok(BashParseResult { commands })
}

fn split_commands(input: &str) -> Vec<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::<String>::new();
    let mut start = 0usize;
    let mut i = 0usize;
    let mut single = false;
    let mut double = false;
    let mut back = false;

    while i < bytes.len() {
        let c = bytes[i] as char;
        let n = if i + 1 < bytes.len() {
            bytes[i + 1] as char
        } else {
            '\0'
        };

        if !single && !double && c == '\\' {
            i = (i + 2).min(bytes.len());
            continue;
        }
        if !double && !back && c == '\'' {
            single = !single;
            i += 1;
            continue;
        }
        if !single && !back && c == '"' {
            double = !double;
            i += 1;
            continue;
        }
        if !single && !double && c == '`' {
            back = !back;
            i += 1;
            continue;
        }
        if !single && !double && !back {
            let is_pair = (c == '&' && n == '&') || (c == '|' && n == '|');
            let is_single = c == ';' || c == '\n' || c == '|';
            if is_pair || is_single {
                let raw = input[start..i].trim();
                if !raw.is_empty() {
                    out.push(raw.to_string());
                }
                i += if is_pair { 2 } else { 1 };
                start = i;
                continue;
            }
        }
        i += 1;
    }

    let tail = input[start..].trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

fn split_words(command: &str) -> Vec<String> {
    let bytes = command.as_bytes();
    let mut out = Vec::<String>::new();
    let mut token = String::new();
    let mut i = 0usize;
    let mut single = false;
    let mut double = false;

    while i < bytes.len() {
        let c = bytes[i] as char;
        if !single && c == '\\' {
            if i + 1 < bytes.len() {
                token.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if !double && c == '\'' {
            single = !single;
            i += 1;
            continue;
        }
        if !single && c == '"' {
            double = !double;
            i += 1;
            continue;
        }
        if !single && !double && c.is_whitespace() {
            if !token.is_empty() {
                out.push(std::mem::take(&mut token));
            }
            i += 1;
            continue;
        }
        token.push(c);
        i += 1;
    }

    if !token.is_empty() {
        out.push(token);
    }
    out
}

fn is_assignment(token: &str) -> bool {
    let Some(eq) = token.find('=') else {
        return false;
    };
    let key = &token[..eq];
    if key.is_empty() {
        return false;
    }
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiple_commands() {
        let out = parse_bash_command("echo foo && echo bar".to_string()).expect("parse");
        assert_eq!(out.commands.len(), 2);
        assert_eq!(out.commands[0].text, "echo foo");
        assert_eq!(out.commands[1].text, "echo bar");
        assert_eq!(out.commands[0].command, vec!["echo", "foo"]);
    }

    #[test]
    fn skips_env_prefix() {
        let out = parse_bash_command("A=1 B=2 git status".to_string()).expect("parse");
        assert_eq!(out.commands.len(), 1);
        assert_eq!(out.commands[0].command, vec!["git", "status"]);
    }
}

