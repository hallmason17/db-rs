/* Copyright (C) 2026 Mason Hall.
 *
 * This file is part of db-rs.
 *
 * db-rs is free software: you can redistribute it and/or modify it under the
 * terms of the GNU General Public License as published by the Free Software
 * Foundation, either version 3 of the License, or (at your option) any later
 * version.
 *
 * db-rs is distributed in the hope that it will be useful, but WITHOUT ANY
 * WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along with
 * db-rs. If not, see <https://www.gnu.org/licenses/>.
 */
use std::borrow::Cow;
use std::fmt::Display;

use crate::tables::TupleRef;
use crate::value::Value::{Blob, Boolean, Float, Int, Null, VarChar};
use crate::value::ValueRef;
use crate::{
    error::{Error, Result},
    tables::Tuple,
    value::Value,
};

#[derive(Debug)]
pub enum Expr {
    And(Box<Expr>, Box<Expr>),
    AttrRef(usize),
    Constant(Value),
    Equal(Box<Expr>, Box<Expr>),
    GreaterThan(Box<Expr>, Box<Expr>),
    IsNull(Box<Expr>),
    IsNotNull(Box<Expr>),
    LessThan(Box<Expr>, Box<Expr>),
    Like(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Subtract(Box<Expr>, Box<Expr>),
    Multiply(Box<Expr>, Box<Expr>),
    Divide(Box<Expr>, Box<Expr>),
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
            Self::IsNull(l) => write!(f, "{l} IS NULL"),
            Self::IsNotNull(l) => write!(f, "{l} IS NOT NULL"),
            Self::Or(l, r) => write!(f, "{l} OR {r}"),
            Self::Not(e) => write!(f, "NOT {e}"),
            Self::Add(l, r) => write!(f, "{l} + {r}"),
            Self::Subtract(l, r) => write!(f, "{l} - {r}"),
            Self::Multiply(l, r) => write!(f, "{l} * {r}"),
            Self::Divide(l, r) => write!(f, "{l} / {r}"),
        }
    }
}

impl Expr {
    pub fn evaluate(&self, tuple: Option<&Tuple>) -> Result<Value> {
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
                (_, _) => Err(Error::InvalidComparison(format!(
                    "cannot eval '{l} AND {r}'"
                ))),
            },
            Self::AttrRef(r) => Ok(tuple.and_then(|t| t.values.get(*r)).cloned().unwrap()),
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
                (_, _) => Err(Error::InvalidComparison(format!("cannot eval '{l} = {r}'"))),
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
                (_, _) => Err(Error::InvalidComparison(format!(
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
                (_, _) => Err(Error::InvalidComparison(format!("cannot eval '{l} > {r}'"))),
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
                (_, _) => Err(Error::InvalidComparison(format!("cannot eval '{l} < {r}'"))),
            },
            Self::Not(r) => match r.evaluate(tuple)? {
                Boolean(b) => Ok(Boolean(!b)),
                Null => Ok(Null),
                _ => Err(Error::InvalidComparison(format!("cannot eval 'NOT {r}'"))),
            },
            Self::IsNull(r) => match r.evaluate(tuple)? {
                Null => Ok(Boolean(true)),
                Boolean(_) | Int(_) | Float(_) | Blob(_) | VarChar(_) => Ok(Boolean(false)),
            },
            Self::IsNotNull(r) => match r.evaluate(tuple)? {
                Null => Ok(Boolean(false)),
                Boolean(_) | Int(_) | Float(_) | Blob(_) | VarChar(_) => Ok(Boolean(true)),
            },
            Self::Like(_l, _r) => todo!(),
            Self::Add(l, r) => match (l.evaluate(tuple)?, r.evaluate(tuple)?) {
                (Int(l), Int(r)) => Ok(Int(l + r)),
                (Float(l), Float(r)) => Ok(Float(l + r)),
                (Int(l), Float(r)) => Ok(Float((l as f32) + r)),
                (Float(l), Int(r)) => Ok(Float(l + (r as f32))),
                (VarChar(l), VarChar(r)) => {
                    let mut s = String::with_capacity(l.len() + r.len());
                    s.push_str(&l);
                    s.push_str(&r);
                    Ok(VarChar(s.into()))
                }
                (Null, _) | (_, Null) => Ok(Null),
                (_, _) => Err(Error::InvalidComparison(format!("cannot eval '{l} + {r}'"))),
            },
            Self::Subtract(l, r) => match (l.evaluate(tuple)?, r.evaluate(tuple)?) {
                (Int(l), Int(r)) => Ok(Int(l - r)),
                (Float(l), Float(r)) => Ok(Float(l - r)),
                (Int(l), Float(r)) => Ok(Float((l as f32) - r)),
                (Float(l), Int(r)) => Ok(Float(l - (r as f32))),
                (Null, _) | (_, Null) => Ok(Null),
                (_, _) => Err(Error::InvalidComparison(format!("cannot eval '{l} - {r}'"))),
            },
            Self::Multiply(l, r) => match (l.evaluate(tuple)?, r.evaluate(tuple)?) {
                (Int(l), Int(r)) => Ok(Int(l * r)),
                (Float(l), Float(r)) => Ok(Float(l * r)),
                (Int(l), Float(r)) => Ok(Float((l as f32) * r)),
                (Float(l), Int(r)) => Ok(Float(l * (r as f32))),
                (Null, _) | (_, Null) => Ok(Null),
                (_, _) => Err(Error::InvalidComparison(format!("cannot eval '{l} * {r}'"))),
            },
            Self::Divide(l, r) => match (l.evaluate(tuple)?, r.evaluate(tuple)?) {
                (Int(l), Int(r)) => Ok(Int(l / r)),
                (Float(l), Float(r)) => Ok(Float(l / r)),
                (Int(l), Float(r)) => Ok(Float((l as f32) / r)),
                (Float(l), Int(r)) => Ok(Float(l / (r as f32))),
                (Null, _) | (_, Null) => Ok(Null),
                (_, _) => Err(Error::InvalidComparison(format!("cannot eval '{l} / {r}'"))),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{expr::Expr, tables::Tuple, value::Value};

    #[test]
    fn add_ints() {
        let expr = Expr::Add(
            Box::new(Expr::Constant(Value::Int(1))),
            Box::new(Expr::Constant(Value::Int(1))),
        );
        assert_eq!(expr.evaluate(None).unwrap(), Value::Int(2))
    }

    #[test]
    fn add_strings() {
        let expr = Expr::Add(
            Box::new(Expr::Constant(Value::VarChar("hello ".into()))),
            Box::new(Expr::Constant(Value::VarChar("world".into()))),
        );
        assert_eq!(
            expr.evaluate(None).unwrap(),
            Value::VarChar("hello world".into())
        )
    }

    #[test]
    fn divide_int_float() {
        let expr = Expr::Divide(
            Box::new(Expr::Constant(Value::Float(1.5))),
            Box::new(Expr::Constant(Value::Int(2))),
        );
        assert_eq!(expr.evaluate(None).unwrap(), Value::Float(0.75))
    }

    #[test]
    fn sub_constants() {
        let expr = Expr::Subtract(
            Box::new(Expr::Constant(Value::Int(1))),
            Box::new(Expr::Constant(Value::Int(1))),
        );
        assert_eq!(expr.evaluate(None).unwrap(), Value::Int(0))
    }

    #[test]
    fn mult_constants() {
        let expr = Expr::Multiply(
            Box::new(Expr::Constant(Value::Int(1))),
            Box::new(Expr::Constant(Value::Int(1))),
        );
        assert_eq!(expr.evaluate(None).unwrap(), Value::Int(1))
    }

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
            Box::new(Expr::Constant(Value::VarChar(
                format!("hello world").into(),
            ))),
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
        let tuple = Tuple::new(&[Value::Int(1), Value::Int(10)]);
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
            Tuple::new(&[Value::Int(1), Value::Int(10)]),
            Tuple::new(&[Value::Int(2), Value::Int(10)]),
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
            Tuple::new(&[Value::Int(1), Value::Int(10)]),
            Tuple::new(&[Value::Int(2), Value::Int(10)]),
        ];

        let results = tuples
            .iter()
            .map(|tuple| expr.evaluate(Some(tuple)).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results, vec![Value::Boolean(true), Value::Boolean(false)])
    }
}
