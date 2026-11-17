use std::mem;

use crate::ShellErrors;
use crate::ShlResult;

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub text: String,
    pub quoted: bool,
    pub literal: bool,
}

impl Token {
    pub fn is_operator(&self, op: &str) -> bool {
        !self.quoted && self.text == op
    }
}

pub fn tokenize(input: &str) -> ShlResult<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut literal = false;
    let mut started = false;

    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            ' ' | '\t' => {
                flush(
                    &mut tokens,
                    &mut current,
                    &mut quoted,
                    &mut literal,
                    &mut started,
                );
            }
            '\'' => {
                quoted = true;
                literal = true;
                started = true;
                let mut closed = false;
                while let Some(c) = chars.next() {
                    if c == '\'' {
                        closed = true;
                        break;
                    }
                    current.push(c);
                }
                if !closed {
                    return Err(ShellErrors::InvalidCommand(format!(
                        "unmatched single quote"
                    )));
                }
            }
            '"' => {
                quoted = true;
                started = true;
                let mut closed = false;
                while let Some(c) = chars.next() {
                    if c == '"' {
                        closed = true;
                        break;
                    }
                    if c == '\\' {
                        if let Some(n) = chars.next() {
                            current.push(n);
                            continue;
                        }
                    }
                    current.push(c);
                }
                if !closed {
                    return Err(ShellErrors::InvalidCommand(format!(
                        "unmatched double quote"
                    )));
                }
            }
            '\\' => {
                started = true;
                if let Some(n) = chars.next() {
                    current.push(n);
                }
            }
            '2' if !started && chars.peek() == Some(&'>') => {
                chars.next();
                flush(
                    &mut tokens,
                    &mut current,
                    &mut quoted,
                    &mut literal,
                    &mut started,
                );
                tokens.push(operator("2>"));
            }
            '<' => {
                flush(
                    &mut tokens,
                    &mut current,
                    &mut quoted,
                    &mut literal,
                    &mut started,
                );
                tokens.push(operator("<"));
            }
            '>' => {
                let text = if chars.peek() == Some(&'>') {
                    chars.next();
                    ">>"
                } else {
                    ">"
                };
                flush(
                    &mut tokens,
                    &mut current,
                    &mut quoted,
                    &mut literal,
                    &mut started,
                );
                tokens.push(operator(text));
            }
            '|' => {
                flush(
                    &mut tokens,
                    &mut current,
                    &mut quoted,
                    &mut literal,
                    &mut started,
                );
                tokens.push(operator("|"));
            }
            _ => {
                started = true;
                current.push(ch);
            }
        }
    }

    if started {
        tokens.push(Token {
            text: current,
            quoted,
            literal,
        });
    }
    Ok(tokens)
}

fn operator(text: &str) -> Token {
    Token {
        text: text.to_string(),
        quoted: false,
        literal: false,
    }
}

fn flush(
    tokens: &mut Vec<Token>,
    current: &mut String,
    quoted: &mut bool,
    literal: &mut bool,
    started: &mut bool,
) {
    if !*started {
        return;
    }

    tokens.push(Token {
        text: mem::take(current),
        quoted: *quoted,
        literal: *literal,
    });
    *quoted = false;
    *literal = false;
    *started = false;
}

#[cfg(test)]
mod tests {
    use crate::lexer::{Token, tokenize};

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
