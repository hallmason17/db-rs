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
