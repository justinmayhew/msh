pub fn parse_line(line: &str) -> Vec<String> {
    let mut argv = Vec::new();

    let mut arg = String::new();
    let mut in_quote = None;

    for c in line.chars() {
        if c == '\'' || c == '"' {
            match in_quote {
                Some(quote) => {
                    if c == quote {
                        in_quote = None;
                        continue;
                    }
                }
                None => {
                    in_quote = Some(c);
                    continue;
                }
            }
        }

        if c.is_whitespace() && in_quote.is_none() {
            if !arg.is_empty() {
                argv.push(arg.clone());
                arg.clear();
            }
            continue;
        }

        arg.push(c);
    }

    if !arg.is_empty() {
        argv.push(arg);
    }

    argv
}

#[cfg(test)]
mod tests {
    use super::parse_line;

    #[test]
    fn simple() {
        assert_eq!(parse_line("ls\n"), vec!["ls".to_string()]);
    }

    #[test]
    fn empty() {
        let empty: Vec<String> = Vec::new();
        assert_eq!(parse_line("\n"), empty);
    }

    #[test]
    fn arguments() {
        assert_eq!(
            parse_line("cat /etc/hosts /etc/passwd\n"),
            vec![
                "cat".to_string(),
                "/etc/hosts".to_string(),
                "/etc/passwd".to_string(),
            ],
        );
    }

    #[test]
    fn single_quotes() {
        assert_eq!(
            parse_line("echo '\"hello\" world'\n"),
            vec!["echo".to_string(), "\"hello\" world".to_string()],
        );
    }

    #[test]
    fn double_quotes() {
        assert_eq!(
            parse_line("echo \"hello 'world'\"\n"),
            vec!["echo".to_string(), "hello 'world'".to_string()],
        );
    }

    #[test]
    fn ignores_consecutive_spaces() {
        assert_eq!(
            parse_line("/bin/echo 1  2   3 '    4'5\n"),
            vec![
                "/bin/echo".to_string(),
                "1".to_string(),
                "2".to_string(),
                "3".to_string(),
                "    45".to_string(),
            ],
        );
    }

    #[test]
    fn ignores_leading_and_trailing_spaces() {
        assert_eq!(parse_line("  cat   \n"), vec!["cat".to_string()]);
    }
}
