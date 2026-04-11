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

use crate::{AdapterDialect, DinocoValue};
use std::collections::HashSet;
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

    fn build_count(&self, stmt: &SelectStatement) -> Query {
        let (inner_sql, params) = self.build_select(stmt);
        let mut sql = String::with_capacity(inner_sql.len() + 64);

        let _ = write!(sql, "SELECT COUNT(*) FROM ({inner_sql}) AS {}", self.identifier("__dinoco_count"));

        (sql, params)
    }

    fn build_partitioned_select(
        &self,
        stmt: &SelectStatement,
        partition_column: &str,
        row_number_alias: &str,
    ) -> Query {
        let mut sql = String::with_capacity(512);
        let mut params = Vec::new();

        sql.push_str("SELECT * FROM (SELECT ");

        if stmt.select.is_empty() {
            sql.push('*');
        } else {
            push_joined(&mut sql, &stmt.select, ", ", |buf, column| {
                render_query_identifier_into(self, column, buf);
            });
        }

        sql.push_str(", ROW_NUMBER() OVER (PARTITION BY ");
        render_query_identifier_into(self, partition_column, &mut sql);

        sql.push_str(" ORDER BY ");

        if stmt.order_by.is_empty() {
            if let Some(first_column) = stmt.select.first() {
                render_query_identifier_into(self, first_column, &mut sql);
            } else {
                render_query_identifier_into(self, partition_column, &mut sql);
            }
        } else {
            push_joined(&mut sql, &stmt.order_by, ", ", |buf, (column, direction)| {
                let dir = match direction {
                    OrderDirection::Asc => "ASC",
                    OrderDirection::Desc => "DESC",
                };

                let _ = write!(buf, "{} {}", self.identifier(column), dir);
            });
        }

        let _ = write!(sql, ") AS {} FROM ", self.identifier(row_number_alias));
        render_query_identifier_into(self, &stmt.from, &mut sql);

        if !stmt.conditions.is_empty() {
            sql.push_str(" WHERE ");
            render_condition_group_into(self, &stmt.conditions, &mut params, " AND ", &mut sql);
        }

        let _ = write!(sql, ") AS {} WHERE ", self.identifier("__dinoco_partitioned"));

        let row_alias = self.identifier(row_number_alias);

        match (stmt.skip, stmt.limit) {
            (Some(skip), Some(limit)) => {
                params.push(DinocoValue::Integer(skip as i64));
                let lower = self.bind_param(params.len());
                params.push(DinocoValue::Integer((skip + limit) as i64));
                let upper = self.bind_param(params.len());
                let _ = write!(sql, "{row_alias} > {lower} AND {row_alias} <= {upper}");
            }
            (Some(skip), None) => {
                params.push(DinocoValue::Integer(skip as i64));
                let lower = self.bind_param(params.len());
                let _ = write!(sql, "{row_alias} > {lower}");
            }
            (None, Some(limit)) => {
                params.push(DinocoValue::Integer(limit as i64));
                let upper = self.bind_param(params.len());
                let _ = write!(sql, "{row_alias} <= {upper}");
            }
            (None, None) => sql.push_str("1 = 1"),
        }

        sql.push_str(" ORDER BY ");
        render_query_identifier_into(self, outer_partition_column(partition_column), &mut sql);
        let _ = write!(sql, ", {}", row_alias);

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
                b.push_str(&self.bind_value(params.len(), value));
            });

            buf.push(')');
        });

        if !stmt.returning.is_empty() {
            sql.push_str(" RETURNING ");

            push_joined(&mut sql, &stmt.returning, ", ", |buf, column| {
                render_query_identifier_into(self, column, buf);
            });
        }

        (sql, params)
    }

    fn build_update(&self, stmt: &UpdateStatement) -> Query {
        let mut sql = String::with_capacity(256);
        let mut params = Vec::new();

        sql.push_str("UPDATE ");

        render_query_identifier_into(self, &stmt.table, &mut sql);

        if stmt.target.is_some() {
            sql.push_str(" AS ");
            sql.push_str(&self.identifier("__dinoco_update"));
        }

        sql.push_str(" SET ");

        if stmt.batches.is_empty() {
            let assignments = merge_update_assignments(&stmt.sets);

            push_joined(&mut sql, &assignments, ", ", |buf, (column, assignments)| {
                render_query_identifier_into(self, column, buf);
                buf.push_str(" = ");
                render_update_assignment_chain_into(self, column, assignments, &mut params, buf);
            });

            if let Some(target) = &stmt.target {
                sql.push_str(" WHERE EXISTS (SELECT 1 FROM (SELECT ");
                push_joined(&mut sql, &target.primary_keys, ", ", |buf, column| {
                    render_query_identifier_into(self, column, buf);
                });
                sql.push_str(" FROM ");
                render_query_identifier_into(self, &stmt.table, &mut sql);

                if !stmt.conditions.is_empty() {
                    sql.push_str(" WHERE ");
                    render_condition_group_into(self, &stmt.conditions, &mut params, " AND ", &mut sql);
                }

                sql.push_str(" LIMIT 1) AS ");
                let target_alias = self.identifier("__dinoco_target");
                sql.push_str(&target_alias);
                sql.push_str(" WHERE ");

                push_joined(&mut sql, &target.primary_keys, " AND ", |buf, column| {
                    let _ = write!(
                        buf,
                        "{}.{} = {}.{}",
                        self.identifier("__dinoco_update"),
                        self.identifier(column),
                        target_alias,
                        self.identifier(column)
                    );
                });
                sql.push(')');
            } else if !stmt.conditions.is_empty() {
                sql.push_str(" WHERE ");

                render_condition_group_into(self, &stmt.conditions, &mut params, " AND ", &mut sql);
            }
            return (sql, params);
        }

        let mut columns = Vec::new();
        let mut seen_columns = HashSet::new();

        for batch in &stmt.batches {
            for (column, _) in &batch.values {
                if seen_columns.insert(column.as_str()) {
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

                    let _ = write!(buf, " THEN {}", self.bind_value(params.len(), value));
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

fn outer_partition_column(partition_column: &str) -> &str {
    partition_column.rsplit('.').next().unwrap_or(partition_column)
}

impl<T: AdapterDialect> QueryBuilder for T {}

fn merge_update_assignments(assignments: &[crate::UpdateAssignment]) -> Vec<(String, Vec<crate::UpdateOperation>)> {
    let mut grouped = Vec::<(String, Vec<crate::UpdateOperation>)>::new();

    for assignment in assignments {
        if let Some((_, operations)) = grouped.iter_mut().find(|(column, _)| column == &assignment.column) {
            operations.push(assignment.operation.clone());
            continue;
        }

        grouped.push((assignment.column.clone(), vec![assignment.operation.clone()]));
    }

    grouped
}

fn render_update_assignment_chain_into<D: AdapterDialect + ?Sized>(
    dialect: &D,
    column: &str,
    assignments: &[crate::UpdateOperation],
    params: &mut Vec<DinocoValue>,
    buf: &mut String,
) {
    let mut expr = dialect.identifier(column);

    for assignment in assignments {
        match assignment {
            crate::UpdateOperation::Set(value) => {
                params.push(value.clone());
                expr = dialect.bind_value(params.len(), value);
            }
            crate::UpdateOperation::Increment(value) => {
                params.push(value.clone());
                expr = format!("({expr} + {})", dialect.bind_value(params.len(), value));
            }
            crate::UpdateOperation::Decrement(value) => {
                params.push(value.clone());
                expr = format!("({expr} - {})", dialect.bind_value(params.len(), value));
            }
            crate::UpdateOperation::Multiply(value) => {
                params.push(value.clone());
                expr = format!("({expr} * {})", dialect.bind_value(params.len(), value));
            }
            crate::UpdateOperation::Division(value) => {
                params.push(value.clone());
                let cast_expr = dialect.cast_numeric_for_division(&expr);
                expr = format!("({cast_expr} / {})", dialect.bind_value(params.len(), value));
            }
        }
    }

    buf.push_str(&expr);
}
