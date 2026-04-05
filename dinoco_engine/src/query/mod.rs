mod data;
mod delete;
mod helpers;
mod insert;
mod select;
mod update;

pub use data::*;
pub use delete::*;
pub use helpers::*;
pub use insert::*;
pub use select::*;
pub use update::*;

use crate::AdapterDialect;
use std::fmt::Write;

pub trait QueryBuilder: AdapterDialect {
    fn build_select(&self, stmt: &SelectStatement) -> Query {
        let mut sql = String::with_capacity(256);
        let mut params = Vec::new();

        sql.push_str("SELECT ");

        if stmt.select.is_empty() {
            sql.push('*');
        } else {
            push_joined(&mut sql, &stmt.select, ", ", |buf, column| {
                render_query_identifier_into(self, column, buf);
            });
        }

        sql.push_str(" FROM ");
        render_query_identifier_into(self, &stmt.from, &mut sql);

        if !stmt.conditions.is_empty() {
            sql.push_str(" WHERE ");
            render_condition_group_into(self, &stmt.conditions, &mut params, " AND ", &mut sql);
        }

        if !stmt.order_by.is_empty() {
            sql.push_str(" ORDER BY ");

            push_joined(&mut sql, &stmt.order_by, ", ", |buf, (column, direction)| {
                let dir = match direction {
                    OrderDirection::Asc => "ASC",
                    OrderDirection::Desc => "DESC",
                };

                let _ = write!(buf, "{} {}", self.identifier(column), dir);
            });
        }

        append_limit_skip(self, &mut sql, &mut params, stmt.limit, stmt.skip);

        (sql, params)
    }

    fn build_insert(&self, stmt: &InsertStatement) -> Query {
        let mut sql = String::with_capacity(256);
        let mut params = Vec::new();

        let _ = write!(sql, "INSERT INTO {} (", self.identifier(&stmt.table));

        push_joined(&mut sql, &stmt.columns, ", ", |buf, col| {
            render_query_identifier_into(self, col, buf);
        });

        sql.push_str(") VALUES ");

        push_joined(&mut sql, &stmt.rows, ", ", |buf, row| {
            buf.push('(');

            push_joined(buf, row, ", ", |b, value| {
                params.push(value.clone());
                b.push_str(&self.bind_param(params.len()));
            });

            buf.push(')');
        });

        (sql, params)
    }

    fn build_update(&self, stmt: &UpdateStatement) -> Query {
        let mut sql = String::with_capacity(256);
        let mut params = Vec::new();

        sql.push_str("UPDATE ");

        render_query_identifier_into(self, &stmt.table, &mut sql);

        sql.push_str(" SET ");

        if stmt.batches.is_empty() {
            push_joined(&mut sql, &stmt.sets, ", ", |buf, (column, value)| {
                params.push(value.clone());

                render_query_identifier_into(self, column, buf);

                let _ = write!(buf, " = {}", self.bind_param(params.len()));
            });

            if !stmt.conditions.is_empty() {
                sql.push_str(" WHERE ");

                render_condition_group_into(self, &stmt.conditions, &mut params, " AND ", &mut sql);
            }
            return (sql, params);
        }

        let mut columns = Vec::<String>::new();

        for batch in &stmt.batches {
            for (column, _) in &batch.values {
                if !columns.contains(column) {
                    columns.push(column.clone());
                }
            }
        }

        push_joined(&mut sql, &columns, ", ", |buf, column| {
            render_query_identifier_into(self, column, buf);
            buf.push_str(" = CASE");

            for batch in &stmt.batches {
                if let Some((_, value)) = batch.values.iter().find(|(bc, _)| bc == column) {
                    buf.push_str(" WHEN ");

                    render_condition_group_into(self, &batch.conditions, &mut params, " AND ", buf);

                    params.push(value.clone());

                    let _ = write!(buf, " THEN {}", self.bind_param(params.len()));
                }
            }

            buf.push_str(" ELSE ");

            render_query_identifier_into(self, column, buf);

            buf.push_str(" END");
        });

        sql.push_str(" WHERE ");

        push_joined(&mut sql, &stmt.batches, " OR ", |buf, batch| {
            buf.push('(');

            render_condition_group_into(self, &batch.conditions, &mut params, " AND ", buf);

            buf.push(')');
        });

        (sql, params)
    }

    fn build_delete(&self, stmt: &DeleteStatement) -> Query {
        let mut sql = String::with_capacity(128);
        let mut params = Vec::new();

        sql.push_str("DELETE FROM ");
        render_query_identifier_into(self, &stmt.table, &mut sql);

        if !stmt.batches.is_empty() {
            sql.push_str(" WHERE ");
            push_joined(&mut sql, &stmt.batches, " OR ", |buf, batch| {
                buf.push('(');

                render_condition_group_into(self, batch, &mut params, " AND ", buf);

                buf.push(')');
            });
        } else if !stmt.conditions.is_empty() {
            sql.push_str(" WHERE ");

            render_condition_group_into(self, &stmt.conditions, &mut params, " AND ", &mut sql);
        }

        (sql, params)
    }
}

impl<T: AdapterDialect> QueryBuilder for T {}
