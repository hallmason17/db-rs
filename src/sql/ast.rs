use crate::sql::types::Token;

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum Ast<'a> {
    Expression {
        literal: Token<'a>,
        binary_op: Box<Ast<'a>>,
    },
    Binary {
        op: Token<'a>,
        left: Box<Ast<'a>>,
        right: Box<Ast<'a>>,
    },
    Select {
        cols: Vec<Ast<'a>>,
        from: Token<'a>,
        r#where: Option<Box<Ast<'a>>>,
    },
    Insert {},
    Update {},
}
