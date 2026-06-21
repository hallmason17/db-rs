/* Copyright (C) 2026 Mason Hall.
 *
 * GNU General Public License v3.0+ (see COPYING or https://www.gnu.org/licenses/gpl-3.0.txt)
 */
use crate::error;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Token<'a> {
    pub kind: TokenType,
    pub data: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TokenType {
    Unknown,
    Ident,
    StringLit,
    Numeric,
    Star,

    Select,
    Update,
    Delete,
    Insert,
    Return,
    From,
    Where,
    In,
    Else,
    Set,
    Create,
    Table,
    Values,
    While,
    Index,
    True,
    False,
    Unique,
    Distinct,
    Varchar,
    Int,
    Float,
    Double,
    Transaction,
    And,
    As,
    Asc,
    Begin,
    Boolean,
    By,
    Order,
    Group,
    Commit,
    Rollback,
    Default,
    Desc,
    Drop,
    Exists,
    Having,
    Into,
    Like,
    Limit,
    NaN,
    Null,
    Not,
    Offset,
    Primary,
    Foreign,
    Or,

    LParen,
    RParen,
    Comma,
    SingleQuote,
    DoubleQuote,
    SemiColon,

    Div,
    DoubleDash,
    Plus,
    Minus,
    GreaterThan,
    GreaterThanOrEq,
    LessThan,
    LessThanOrEq,
    Eq,
    NotEq,
    DoubleEq,

    Eof,
}

impl TryFrom<&str> for TokenType {
    type Error = error::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(match value {
            "select" => Self::Select,
            "update" => Self::Update,
            "delete" => Self::Delete,
            "insert" => Self::Insert,
            "return" => Self::Return,
            "from" => Self::From,
            "where" => Self::Where,
            "in" => Self::In,
            "else" => Self::Else,
            "set" => Self::Set,
            "create" => Self::Create,
            "table" => Self::Table,
            "values" => Self::Values,
            "while" => Self::While,
            "index" => Self::Index,
            "true" => Self::True,
            "false" => Self::False,
            "unique" => Self::Unique,
            "distinct" => Self::Distinct,
            "varchar" => Self::Varchar,
            "int" => Self::Int,
            "float" => Self::Float,
            "double" => Self::Double,
            "transaction" => Self::Transaction,
            "and" => Self::And,
            "as" => Self::As,
            "asc" => Self::Asc,
            "begin" => Self::Begin,
            "boolean" => Self::Boolean,
            "by" => Self::By,
            "order" => Self::Order,
            "group" => Self::Group,
            "commit" => Self::Commit,
            "rollback" => Self::Rollback,
            "default" => Self::Default,
            "desc" => Self::Desc,
            "drop" => Self::Drop,
            "exists" => Self::Exists,
            "having" => Self::Having,
            "into" => Self::Into,
            "like" => Self::Like,
            "limit" => Self::Limit,
            "nan" => Self::NaN,
            "null" => Self::Null,
            "not" => Self::Not,
            "offset" => Self::Offset,
            "primary" => Self::Primary,
            "foreign" => Self::Foreign,
            _ => {
                return Err(error::Error::ParseError(
                    "token '{value}' does not exist".into(),
                ));
            }
        })
    }
}
