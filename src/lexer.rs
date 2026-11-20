use crate::{ShellErrors, ShlResult};

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub text: String,
    pub quoted: bool,
    pub literal: bool,
}

impl Token {
    fn unquoted(text: &str) -> Self {
        Self {
            text: text.to_string(),
            quoted: false,
            literal: false,
        }
    }

    pub fn is_operator(&self, op: &str) -> bool {
        !self.quoted && self.text == op
    }
}

pub fn tokenize(input: &str) -> ShlResult<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut current: Option<Token> = None;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        let operator = match ch {
            '2' if current.is_none() && chars.next_if_eq(&'>').is_some() => Some("2>"),
            '>' if chars.next_if_eq(&'>').is_some() => Some(">>"),
            '>' => Some(">"),
            '<' => Some("<"),
            '|' => Some("|"),
            _ => None,
        };

        if matches!(ch, ' ' | '\t') || operator.is_some() {
            tokens.extend(current.take());
            if let Some(operator) = operator {
                tokens.push(Token::unquoted(operator));
            }
            continue;
        }

        let word = current.get_or_insert_with(|| Token::unquoted(""));
        match ch {
            '\'' | '"' => {
                word.quoted = true;
                word.literal |= ch == '\'';
                read_quoted(&mut chars, ch, &mut word.text)?;
            }
            '\\' => word.text.extend(chars.next()),
            _ => word.text.push(ch),
        }
    }

    tokens.extend(current);
    Ok(tokens)
}

fn read_quoted(
    chars: &mut impl Iterator<Item = char>,
    quote: char,
    text: &mut String,
) -> ShlResult<()> {
    while let Some(ch) = chars.next() {
        match ch {
            ch if ch == quote => return Ok(()),
            '\\' if quote == '"' => text.extend(chars.next()),
            _ => text.push(ch),
        }
    }

    let kind = if quote == '\'' { "single" } else { "double" };
    Err(ShellErrors::InvalidCommand(format!(
        "unmatched {kind} quote"
    )))
}

#[cfg(test)]
mod tests {
    use crate::lexer::{Token, tokenize};

    #[test]
    fn empty_quotes_start_a_word_before_stderr_redirection() {
        let tokens = tokenize(r#"''2>out 2>err"#).unwrap();
        let texts: Vec<_> = tokens.iter().map(|token| token.text.as_str()).collect();

        assert_eq!(texts, ["2", ">", "out", "2>", "err"]);
        assert!(tokens[0].quoted && tokens[0].literal);
        assert!(tokens[1].is_operator(">"));
        assert!(tokens[3].is_operator("2>"));
    }

    #[test]
    fn parse_simple_cmd() {
        let cmd = "cat README | grep 'rust'";
        let tokens = tokenize(cmd);
        println!("tokens: {:?}", tokens);
        assert_eq!(
            tokens.unwrap(),
            vec![
                Token {
                    text: "cat".to_string(),
                    quoted: false,
                    literal: false
                },
                Token {
                    text: "README".to_string(),
                    quoted: false,
                    literal: false
                },
                Token {
                    text: "|".to_string(),
                    quoted: false,
                    literal: false
                },
                Token {
                    text: "grep".to_string(),
                    quoted: false,
                    literal: false
                },
                Token {
                    text: "rust".to_string(),
                    quoted: true,
                    literal: true
                },
            ]
        )
    }
}
