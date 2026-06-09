use crate::error::Error::{self, ParseError};
use crate::error::Result;
use crate::sql::ast::Ast;
use crate::sql::types::TokenType;
use crate::sql::{lexer::Lexer, types::Token};
use std::cell::RefCell;
use std::iter::Peekable;
use std::slice::Iter;

pub struct Parser<'a> {
    lexer: RefCell<Lexer<'a>>,
}

impl<'a> Parser<'a> {
    pub fn new() -> Self {
        Self {
            lexer: Lexer::new().into(),
        }
    }
    pub fn parse(&self, sql: &'a str) -> Result<Ast> {
        let mut lexer = self.lexer.borrow_mut();
        let tokens = lexer.tokenize(sql)?;
        let mut it = tokens.iter().peekable();
        match it.next() {
            Some(tok) => match tok.kind {
                TokenType::Select => self.parse_select(it),
                _ => Err(Error::ParseError("Unknown".into())),
            },
            None => Err(Error::ParseError("empty sql string!".into())),
        }
    }

    fn expect(&self, expected: TokenType, it: &mut Peekable<Iter<Token>>) -> Result<()> {
        let tok = it.next();
        if let Some(tok) = tok
            && tok.kind != expected
        {
            return Err(Error::ParseError(format!(
                "expected {:?}, found {:?}",
                expected, tok
            )));
        }
        Ok(())
    }

    fn parse_expr(&self, it: &mut Peekable<Iter<Token>>) -> Result<Ast> {
        ./
    }

    fn parse_select(&self, mut it: Peekable<Iter<Token>>) -> Result<Ast> {
        Ok(Ast::Select {
            cols: self.parse_cols(&mut it)?,
            from: self.parse_from(&mut it)?,
            r#where: self.parse_where(&mut it)?,
        })
    }
    fn parse_cols(&self, it: &mut Peekable<Iter<Token>>) -> Result<Vec<Ast>> {
        let mut cols = vec![];
        while let Some(token) = it.next()
            && token.kind != TokenType::From
        {
            cols.push(self.parse_expr(it)?);
        }
        Ok(cols)
    }
    fn parse_from(&self, mut _it: &mut Peekable<Iter<Token>>) -> Result<Token> {
        Ok(Token {
            kind: TokenType::Ident,
            data: "hello",
        })
    }
    fn parse_where(&self, it: &mut Peekable<Iter<Token>>) -> Result<Option<Box<Ast>>> {
        if let Some(token) = it.next()
            && token.kind != TokenType::Where
        {
            return Ok(None);
        }
        Ok(Some(Box::new(self.parse_expr(it)?)))
    }
}

impl<'a> Default for Parser<'a> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_select() {
        let parser = Parser::new();
        let ast = parser.parse("select * from users;").unwrap();
        assert_eq!(ast, Ast {});
    }
}
