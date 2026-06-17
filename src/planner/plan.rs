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
use crate::{
    expr::Expr,
    tables::{Table, TableSchema},
};

#[derive(Debug)]
pub enum SqlStatement {
    Select {
        cols: Vec<Expr>,
        tables: Vec<Table>,
        filter: Option<Expr>,
    },
}

#[derive(Debug)]
pub enum SortOrder {
    Ascending,
    Descending,
}

#[derive(Debug)]
pub enum QueryPlan {
    CreateTable { schema: TableSchema },
    Insert,
    Update,
    Delete,
    Select(PlanNode),
}

#[derive(Debug)]
pub enum PlanNode {
    Filter {
        children: Vec<PlanNode>,
        pred: Expr,
    },
    SeqScan {
        cols: Vec<Expr>,
        table: Table,
        filter: Option<Expr>,
    },
    Sort {
        children: Vec<PlanNode>,
        order: SortOrder,
    },
}

pub struct Planner {}
impl Planner {
    pub fn new() -> Self {
        Self {}
    }
    pub fn plan(&self, statement: SqlStatement) -> QueryPlan {
        match statement {
            SqlStatement::Select {
                cols,
                tables,
                filter,
            } => QueryPlan::Select(PlanNode::SeqScan {
                cols,
                table: tables.first().unwrap().clone(),
                filter,
            }),
        }
    }
}
impl Default for Planner {
    fn default() -> Self {
        Self::new()
    }
}
