use std::mem;

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
    let chars: Vec<char> = input.chars().collect();
    let mut current = String::new();

    let mut idx = 0;
    let mut in_word = false;
    while idx < chars.len() {
        let ch = chars.get(idx).ok_or_else(|| ShellErrors::EofChars)?;
        match ch {
            ' ' | '\t' => {
                idx += 1;
                if !in_word {
                    continue;
                }

                in_word = false;
                if !current.is_empty() {
                    tokens.push(Token {
                        text: mem::take(&mut current),
                        quoted: false,
                        literal: false,
                    });
                }
            }
            '2' if !in_word && chars.get(idx + 1) == Some(&'>') => {
                tokens.push(Token::unquoted("2>"));
                idx += 2;
            }
            '\'' => {
                let (res, new_idx) = read_quoted(&chars, idx + 1, '\'')?;
                tokens.push(Token {
                    text: res,
                    quoted: true,
                    literal: true,
                });

                idx = new_idx;
            }
            '"' => {
                let (res, new_idx) = read_quoted(&chars, idx + 1, '"')?;
                tokens.push(Token {
                    text: res,
                    quoted: true,
                    literal: true,
                });

                idx = new_idx;
            }
            '<' => {
                tokens.push(Token::unquoted("<"));
                idx += 1;
            }
            '>' => {
                let op = if idx < chars.len() && chars.get(idx + 1) == Some(&'>') {
                    idx += 2;
                    ">>"
                } else {
                    idx += 1;
                    ">"
                };

                tokens.push(Token::unquoted(op));
            }
            '|' => {
                tokens.push(Token::unquoted("|"));
                idx += 1;
            }
            _ => {
                in_word = true;
                current.push(*ch);
                idx += 1;
            }
        }
    }

    if !current.is_empty() {
        tokens.push(Token {
            text: mem::take(&mut current),
            quoted: false,
            literal: false,
        });
    }

    Ok(tokens)
}

fn read_quoted(chars: &Vec<char>, start: usize, quote: char) -> ShlResult<(String, usize)> {
    let mut idx = start;
    let mut result = String::new();
    let mut end = false;
    while idx < chars.len() {
        let v = chars.get(idx).ok_or_else(|| ShellErrors::EofChars)?;
        if v == &quote {
            end = true;
            idx += 1;
            break;
        }

        result.push(*v);
        idx += 1;
    }

    if !end && idx >= chars.len() {
        let kind = if quote == '\'' { "single" } else { "double" };
        Err(ShellErrors::InvalidCommand(format!(
            "unmatched {kind} quote"
        )))
    } else {
        Ok((result, idx))
    }
}

#[cfg(test)]
mod tests {
    use crate::lexer::{Token, tokenize};

    #[test]
    fn empty_quotes_start_a_word_before_stderr_redirection() {
        let tokens = tokenize(r#"'' 2>out 2>err"#).unwrap();
        let texts: Vec<_> = tokens.iter().map(|token| token.text.as_str()).collect();

        assert_eq!(texts, ["", "2>", "out", "2>", "err"]);
        assert!(tokens[0].quoted && tokens[0].literal);
        assert!(tokens[1].is_operator("2>"));
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
