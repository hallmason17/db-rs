use std::fmt::Display;

use crate::{
    error::{DbError, DbResult},
    value::Value,
};

pub enum Expr {
    And(Box<Expr>, Box<Expr>),
    AttrRef(usize),
    Constant(Value),
    Equal(Box<Expr>, Box<Expr>),
    GreaterThan(Box<Expr>, Box<Expr>),
    Is(Box<Expr>, Value),
    LessThan(Box<Expr>, Box<Expr>),
    Like(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}
impl Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::And(l, r) => write!(f, "{l} AND {r}"),
            Self::AttrRef(r) => write!(f, "Attr: {r}"),
            Self::Constant(c) => write!(f, "Const: {:?}", c),
            Self::Equal(l, r) => write!(f, "{l} = {r}"),
            Self::GreaterThan(l, r) => write!(f, "{l} > {r}"),
            Self::LessThan(l, r) => write!(f, "{l} < {r}"),
            Self::Like(l, r) => write!(f, "{l} LIKE {r}"),
            Self::Is(l, Value::Null) => write!(f, "{l} IS NULL"),
            Self::Is(_, _) => panic!("invalid IS"),
            Self::Or(l, r) => write!(f, "{l} OR {r}"),
            Self::Not(e) => write!(f, "NOT {e}"),
        }
    }
}

impl Expr {
    pub fn evaluate(&self, tuple: Option<&[Value]>) -> DbResult<Value> {
        use Value::*;
        match self {
            Self::And(l, r) => match (l.evaluate(tuple)?, r.evaluate(tuple)?) {
                // https://spark.apache.org/docs/4.1.2/sql-ref-null-semantics.html#logical-operators
                (Boolean(l), Boolean(r)) => Ok(Boolean(l && r)),
                (Boolean(b), Null) | (Null, Boolean(b)) => {
                    if b {
                        Ok(Null)
                    } else {
                        Ok(Boolean(b))
                    }
                }
                (Null, Null) => Ok(Null),
                (_, _) => Err(DbError::InvalidComparison(format!(
                    "cannot eval '{l} AND {r}'"
                ))),
            },
            Self::AttrRef(r) => Ok(tuple.and_then(|t| t.get(*r)).cloned().unwrap()),
            Self::Constant(c) => Ok(c.clone()),
            Self::Equal(l, r) => match (l.evaluate(tuple)?, r.evaluate(tuple)?) {
                (Boolean(l), Boolean(r)) => Ok(Boolean(l == r)),
                (Int(l), Int(r)) => Ok(Boolean(l == r)),
                (Float(l), Float(r)) => Ok(Boolean(l == r)),
                (Int(l), Float(r)) => Ok(Boolean(l as f32 == r)),
                (Float(l), Int(r)) => Ok(Boolean(l == r as f32)),
                (VarChar(l), VarChar(r)) => Ok(Boolean(l == r)),
                (Blob(l), Blob(r)) => Ok(Boolean(l == r)),
                (Null, _) | (_, Null) => Ok(Null),
                (_, _) => Err(DbError::InvalidComparison(format!(
                    "cannot eval '{l} = {r}'"
                ))),
            },
            Self::Or(l, r) => match (l.evaluate(tuple)?, r.evaluate(tuple)?) {
                // https://spark.apache.org/docs/4.1.2/sql-ref-null-semantics.html#logical-operators
                (Boolean(l), Boolean(r)) => Ok(Boolean(l || r)),
                (Boolean(b), Null) | (Null, Boolean(b)) => {
                    if b {
                        Ok(Boolean(b))
                    } else {
                        Ok(Null)
                    }
                }
                (Null, Null) => Ok(Null),
                (_, _) => Err(DbError::InvalidComparison(format!(
                    "cannot eval '{l} OR {r}'"
                ))),
            },
            Self::GreaterThan(l, r) => match (l.evaluate(tuple)?, r.evaluate(tuple)?) {
                (Boolean(l), Boolean(r)) => Ok(Boolean(l & !r)),
                (Int(l), Int(r)) => Ok(Boolean(l > r)),
                (Float(l), Float(r)) => Ok(Boolean(l > r)),
                (Int(l), Float(r)) => Ok(Boolean(l as f32 > r)),
                (Float(l), Int(r)) => Ok(Boolean(l > r as f32)),
                (VarChar(l), VarChar(r)) => Ok(Boolean(l > r)),
                (Blob(l), Blob(r)) => Ok(Boolean(l > r)),
                (Null, _) | (_, Null) => Ok(Null),
                (_, _) => Err(DbError::InvalidComparison(format!(
                    "cannot eval '{l} > {r}'"
                ))),
            },
            Self::LessThan(l, r) => match (l.evaluate(tuple)?, r.evaluate(tuple)?) {
                (Boolean(l), Boolean(r)) => Ok(Boolean(!l & r)),
                (Int(l), Int(r)) => Ok(Boolean(l < r)),
                (Float(l), Float(r)) => Ok(Boolean(l < r)),
                (Int(l), Float(r)) => Ok(Boolean((l as f32) < r)),
                (Float(l), Int(r)) => Ok(Boolean(l < (r as f32))),
                (VarChar(l), VarChar(r)) => Ok(Boolean(l < r)),
                (Blob(l), Blob(r)) => Ok(Boolean(l < r)),
                (Null, _) | (_, Null) => Ok(Null),
                (_, _) => Err(DbError::InvalidComparison(format!(
                    "cannot eval '{l} < {r}'"
                ))),
            },
            Self::Not(r) => match r.evaluate(tuple)? {
                Boolean(b) => Ok(Boolean(!b)),
                Null => Ok(Null),
                _ => Err(DbError::InvalidComparison(format!("cannot eval 'NOT {r}'"))),
            },
            Self::Like(_l, _r) => todo!(),
            Self::Is(_l, _r) => todo!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{expr::Expr, value::Value};

    #[test]
    fn eq_constants() {
        let expr = Expr::Equal(
            Box::new(Expr::Constant(Value::Int(1))),
            Box::new(Expr::Constant(Value::Int(1))),
        );
        assert_eq!(expr.evaluate(None).unwrap(), Value::Boolean(true))
    }

    #[test]
    fn eq_wrong_types() {
        let expr = Expr::Equal(
            Box::new(Expr::Constant(Value::Int(1))),
            Box::new(Expr::Constant(Value::VarChar(String::from("hello world")))),
        );
        assert!(expr.evaluate(None).is_err())
    }

    #[test]
    fn eq_cross_types() {
        let expr = Expr::Equal(
            Box::new(Expr::Constant(Value::Int(1))),
            Box::new(Expr::Constant(Value::Float(1.0))),
        );
        assert_eq!(expr.evaluate(None).unwrap(), Value::Boolean(true))
    }

    #[test]
    fn gt_cross_types_true() {
        let expr = Expr::GreaterThan(
            Box::new(Expr::Constant(Value::Int(2))),
            Box::new(Expr::Constant(Value::Float(1.5))),
        );
        assert_eq!(expr.evaluate(None).unwrap(), Value::Boolean(true))
    }

    #[test]
    fn gt_cross_types_false() {
        let expr = Expr::GreaterThan(
            Box::new(Expr::Constant(Value::Int(1))),
            Box::new(Expr::Constant(Value::Float(1.5))),
        );
        assert_eq!(expr.evaluate(None).unwrap(), Value::Boolean(false))
    }

    #[test]
    fn gt_nulls() {
        let expr = Expr::GreaterThan(
            Box::new(Expr::Constant(Value::Null)),
            Box::new(Expr::Constant(Value::Null)),
        );
        assert_eq!(expr.evaluate(None).unwrap(), Value::Null)
    }

    #[test]
    fn basic_or() {
        let expr = Expr::Or(
            Box::new(Expr::Constant(Value::Boolean(true))),
            Box::new(Expr::Constant(Value::Boolean(false))),
        );
        assert_eq!(expr.evaluate(None).unwrap(), Value::Boolean(true))
    }

    #[test]
    fn or_null_false() {
        let expr = Expr::Or(
            Box::new(Expr::Constant(Value::Null)),
            Box::new(Expr::Constant(Value::Boolean(false))),
        );
        assert_eq!(expr.evaluate(None).unwrap(), Value::Null)
    }

    #[test]
    fn or_null_true() {
        let expr = Expr::Or(
            Box::new(Expr::Constant(Value::Null)),
            Box::new(Expr::Constant(Value::Boolean(true))),
        );
        assert_eq!(expr.evaluate(None).unwrap(), Value::Boolean(true))
    }

    #[test]
    fn basic_and() {
        let tuple = [Value::Int(1), Value::Int(10)];
        let expr = Expr::And(
            Box::new(Expr::Equal(
                Box::new(Expr::AttrRef(1)),
                Box::new(Expr::Constant(Value::Int(10))),
            )),
            Box::new(Expr::Equal(
                Box::new(Expr::AttrRef(1)),
                Box::new(Expr::Constant(Value::Int(10))),
            )),
        );
        assert_eq!(expr.evaluate(Some(&tuple)).unwrap(), Value::Boolean(true))
    }

    #[test]
    fn and_null_true() {
        let expr = Expr::And(
            Box::new(Expr::Constant(Value::Null)),
            Box::new(Expr::Constant(Value::Boolean(true))),
        );
        assert_eq!(expr.evaluate(None).unwrap(), Value::Null)
    }

    #[test]
    fn and_null_false() {
        let expr = Expr::And(
            Box::new(Expr::Constant(Value::Null)),
            Box::new(Expr::Constant(Value::Boolean(false))),
        );
        assert_eq!(expr.evaluate(None).unwrap(), Value::Boolean(false))
    }

    #[test]
    fn match_many_tuples() {
        let expr = Expr::And(
            Box::new(Expr::Equal(
                Box::new(Expr::AttrRef(1)),
                Box::new(Expr::Constant(Value::Int(10))),
            )),
            Box::new(Expr::Equal(
                Box::new(Expr::AttrRef(1)),
                Box::new(Expr::Constant(Value::Int(10))),
            )),
        );
        let tuples = [
            [Value::Int(1), Value::Int(10)],
            [Value::Int(2), Value::Int(10)],
        ];

        let results = tuples
            .iter()
            .map(|tuple| expr.evaluate(Some(tuple)).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results, [Value::Boolean(true), Value::Boolean(true)])
    }
    #[test]
    fn match_one_of_many_tuples() {
        let expr = Expr::And(
            Box::new(Expr::Equal(
                Box::new(Expr::AttrRef(0)),
                Box::new(Expr::Constant(Value::Int(1))),
            )),
            Box::new(Expr::Equal(
                Box::new(Expr::AttrRef(1)),
                Box::new(Expr::Constant(Value::Int(10))),
            )),
        );
        let tuples = [
            [Value::Int(1), Value::Int(10)],
            [Value::Int(2), Value::Int(10)],
        ];

        let results = tuples
            .iter()
            .map(|tuple| expr.evaluate(Some(tuple)).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results, vec![Value::Boolean(true), Value::Boolean(false)])
    }
}
