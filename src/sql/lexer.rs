use std::collections::HashMap;

use crate::{
    error::{
        Error::{self, ParseError},
        Result,
    },
    sql::types::{Token, TokenType},
};

pub struct Lexer<'a> {
    keywords: HashMap<String, TokenType>,
    input_text: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new() -> Self {
        let mut keywords = HashMap::new();
        keywords.insert("select".into(), TokenType::Select);
        keywords.insert("from".into(), TokenType::From);
        keywords.insert("where".into(), TokenType::Where);
        keywords.insert("insert".into(), TokenType::Insert);
        keywords.insert("or".into(), TokenType::Or);
        keywords.insert("and".into(), TokenType::And);
        keywords.insert("create".into(), TokenType::Create);
        keywords.insert("values".into(), TokenType::Values);
        keywords.insert("true".into(), TokenType::True);
        keywords.insert("false".into(), TokenType::False);
        keywords.insert("index".into(), TokenType::Index);
        keywords.insert("drop".into(), TokenType::Drop);
        keywords.insert("delete".into(), TokenType::Delete);
        keywords.insert("null".into(), TokenType::Null);
        keywords.insert("exists".into(), TokenType::Exists);
        keywords.insert("transaction".into(), TokenType::Transaction);
        keywords.insert("limit".into(), TokenType::Limit);
        keywords.insert("offset".into(), TokenType::Offset);
        keywords.insert("order".into(), TokenType::Order);
        keywords.insert("by".into(), TokenType::By);
        keywords.insert("group".into(), TokenType::Group);

        Self {
            keywords,
            input_text: "",
            pos: 0,
        }
    }
    pub fn tokenize(&mut self, src: &'a str) -> Result<Vec<Token<'_>>> {
        if !src.is_empty() {
            self.input_text = src;
            self.pos = 0;
        }
        let mut tokens = vec![];
        while self.pos < self.input_text.len() {
            let tok = self.get_next_token()?;
            tokens.push(tok);
        }
        Ok(tokens)
    }

    fn check_keyword(&self, word: &str) -> TokenType {
        let lower = word.to_lowercase();
        match self.keywords.get(&lower) {
            Some(kind) => *kind,
            None => TokenType::Ident,
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(next) = self.input_text.chars().nth(self.pos)
            && next.is_whitespace()
        {
            self.pos += 1;
        }
    }
    fn peek(&self) -> Option<char> {
        self.input_text.chars().nth(self.pos + 1)
    }
    fn atom(&mut self, kind: TokenType, len: usize) -> Token<'a> {
        self.pos += len;
        let data = &self.input_text[self.pos - len..self.pos];
        Token { kind, data }
    }
    fn get_next_token(&mut self) -> Result<Token<'a>> {
        self.skip_whitespace();
        if self.pos >= self.input_text.len() {
            return Ok(Token {
                kind: TokenType::Eof,
                data: "",
            });
        }
        let ch = self.input_text.chars().nth(self.pos).unwrap();
        match ch {
            '+' => Ok(self.atom(TokenType::Plus, 1)),
            '-' => {
                if let Some(next) = self.peek()
                    && next == '-'
                {
                    return Ok(self.atom(TokenType::DoubleDash, 2));
                }
                Ok(self.atom(TokenType::Minus, 1))
            }
            '/' => Ok(self.atom(TokenType::Div, 1)),
            '*' => Ok(self.atom(TokenType::Star, 1)),
            ',' => Ok(self.atom(TokenType::Comma, 1)),
            '(' => Ok(self.atom(TokenType::LParen, 1)),
            ')' => Ok(self.atom(TokenType::RParen, 1)),
            ';' => Ok(self.atom(TokenType::SemiColon, 1)),
            '=' => {
                if let Some(next) = self.peek()
                    && next == '='
                {
                    return Ok(self.atom(TokenType::DoubleEq, 2));
                }
                Ok(self.atom(TokenType::Eq, 1))
            }
            '>' => {
                if let Some(next) = self.peek()
                    && next == '='
                {
                    return Ok(self.atom(TokenType::GreaterThanOrEq, 2));
                }
                Ok(self.atom(TokenType::GreaterThan, 1))
            }
            '<' => {
                if let Some(next) = self.peek() {
                    return match next {
                        '>' => {
                            Ok(self.atom(TokenType::NotEq, 2))
                        }
                        '=' => Ok(self.atom(TokenType::LessThanOrEq, 2)),
                        _ => Err(Error::ParseError("could not parse '<{next}'".into())),
                    }
                }
                Ok(self.atom(TokenType::LessThan, 1))
            }
            '\'' => {
                self.pos += 1;
                let start = self.pos;
                while let Some(next) = self.input_text.chars().nth(self.pos)
                    && next != '\''
                {
                    self.pos += 1;
                }
                let data = &self.input_text[start..self.pos];
                self.pos += 1;
                Ok(Token {
                    kind: TokenType::StringLit,
                    data,
                })
            }
            'a'..='z' | 'A'..='Z' => {
                let start = self.pos;
                while self.pos < self.input_text.len()
                    && self
                        .input_text
                        .chars()
                        .nth(self.pos)
                        .unwrap()
                        .is_alphanumeric()
                {
                    self.pos += 1;
                }
                let data = &self.input_text[start..self.pos];
                let kind = self.check_keyword(data);
                Ok(Token { kind, data })
            }
            '0'..='9' => {
                let start = self.pos;
                while let Some(next) = self.input_text.chars().nth(self.pos)
                    && (next.is_numeric() || next == '.')
                {
                    self.pos += 1;
                }
                let data = &self.input_text[start..self.pos];
                Ok(Token {
                    kind: TokenType::Numeric,
                    data,
                })
            }
            _ => Err(ParseError(format!("Couldn't parse {}", ch))),
        }
    }
}

impl<'a> Default for Lexer<'a> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_basic() {
        let sql = "select (1.0+1)";
        let mut lexer = Lexer::new();
        let tokens = lexer.tokenize(sql).unwrap();

        assert_eq!(
            tokens,
            vec![
                Token {
                    kind: TokenType::Select,
                    data: "select"
                },
                Token {
                    kind: TokenType::LParen,
                    data: "("
                },
                Token {
                    kind: TokenType::Numeric,
                    data: "1.0"
                },
                Token {
                    kind: TokenType::Plus,
                    data: "+"
                },
                Token {
                    kind: TokenType::Numeric,
                    data: "1"
                },
                Token {
                    kind: TokenType::RParen,
                    data: ")"
                },
            ]
        )
    }

    #[test]
    fn tokenize_with_newlines() {
        let sql = "select *\nfrom users\nwhere\r\n\tname = 'mason';";
        let mut lexer = Lexer::new();
        let tokens = lexer.tokenize(sql).unwrap();

        assert_eq!(
            tokens,
            vec![
                Token {
                    kind: TokenType::Select,
                    data: "select"
                },
                Token {
                    kind: TokenType::Star,
                    data: "*"
                },
                Token {
                    kind: TokenType::From,
                    data: "from"
                },
                Token {
                    kind: TokenType::Ident,
                    data: "users"
                },
                Token {
                    kind: TokenType::Where,
                    data: "where"
                },
                Token {
                    kind: TokenType::Ident,
                    data: "name"
                },
                Token {
                    kind: TokenType::Eq,
                    data: "="
                },
                Token {
                    kind: TokenType::StringLit,
                    data: "mason"
                },
                Token {
                    kind: TokenType::SemiColon,
                    data: ";"
                },
            ]
        )
    }

    #[test]
    fn tokenize() {
        let sql = "Select * from users;";
        let mut lexer = Lexer::new();
        let tokens = lexer.tokenize(sql).unwrap();
        assert_eq!(
            tokens,
            vec![
                Token {
                    kind: TokenType::Select,
                    data: "Select"
                },
                Token {
                    kind: TokenType::Star,
                    data: "*"
                },
                Token {
                    kind: TokenType::From,
                    data: "from"
                },
                Token {
                    kind: TokenType::Ident,
                    data: "users"
                },
                Token {
                    kind: TokenType::SemiColon,
                    data: ";"
                },
            ]
        )
    }

    #[test]
    fn tokenize_bigger() {
        let sql = "Select * from users where name = 'mason' or name = 'janice';";
        let mut lexer = Lexer::new();
        let tokens = lexer.tokenize(sql).unwrap();
        assert_eq!(
            tokens,
            vec![
                Token {
                    kind: TokenType::Select,
                    data: "Select"
                },
                Token {
                    kind: TokenType::Star,
                    data: "*"
                },
                Token {
                    kind: TokenType::From,
                    data: "from"
                },
                Token {
                    kind: TokenType::Ident,
                    data: "users"
                },
                Token {
                    kind: TokenType::Where,
                    data: "where"
                },
                Token {
                    kind: TokenType::Ident,
                    data: "name"
                },
                Token {
                    kind: TokenType::Eq,
                    data: "="
                },
                Token {
                    kind: TokenType::StringLit,
                    data: "mason"
                },
                Token {
                    kind: TokenType::Or,
                    data: "or"
                },
                Token {
                    kind: TokenType::Ident,
                    data: "name"
                },
                Token {
                    kind: TokenType::Eq,
                    data: "="
                },
                Token {
                    kind: TokenType::StringLit,
                    data: "janice"
                },
                Token {
                    kind: TokenType::SemiColon,
                    data: ";"
                },
            ]
        )
    }
}
