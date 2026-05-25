use crate::value::Value;

pub enum Expr {
    Constant(Value),
    AttrRef(usize),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Equal(Box<Expr>, Box<Expr>),
    GreaterThan(Box<Expr>, Box<Expr>),
    LessThan(Box<Expr>, Box<Expr>),
    Is(Box<Expr>, Value),
    Like(Box<Expr>, Box<Expr>),
}
