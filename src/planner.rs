// Create Data Frame
//  - Bind
//  - Analyze
//  - Plan
use crate::dataframe::DataFrame;
use crate::expressions::*;

use anyhow::Ok;
use anyhow::Result;
use anyhow::bail;
use sqlparser::ast::GroupByExpr;

use std::borrow::Borrow;
use std::collections::HashMap;
use std::collections::HashSet;
use std::rc::Rc;

use sqlparser::ast;
use sqlparser::ast::Expr;
use sqlparser::ast::Query;
use sqlparser::ast::SelectItem;
use sqlparser::ast::SetExpr;
use sqlparser::ast::Statement;
use std::str::FromStr;

pub struct Planner {}

enum QueryClass {
    Relational,
    Aggregate,
}

/// An analyzed, validated and enriched form of QueryIR
/// an intermediate representation is converted to it's
/// analyzed form by the analyzer and enriched with
/// more information which is useful later down the planning
/// pipeline. This is always created by CONSUMING the relative
/// QueryIR struct
struct AnalyzedQuery {
    class: QueryClass,
    select: Vec<Expression>,
    wherec: Option<Expression>,
    groupby: Vec<Expression>,
    having: Option<Expression>,
    interim_projections: Option<Vec<Expression>>,
    aggregates: Vec<Expression>,
    // only used during aggregate query planning
    outputReferences: Option<Vec<Expression>>,
}

impl Planner {
    pub fn new() -> Self {
        Planner {}
    }

    /// analyzes the intermediate representation of a query
    /// consumes QueryID and produces AnalyzedQuery for use
    /// in planning phase.
    fn analyze(&self, mut ir: QueryIR) -> Result<AnalyzedQuery> {
        // analysis rules
        // - if aggregate expressions present, group by should also be mentioned
        // - if the query is an aggregate query, the non-aggregate projections should be present in group by
        // - if the query is an aggregate query, build column indexes
        // - if interim projections exist, where should exist, otherwise throw error

        // - Create interim projections
        let aggregates: Vec<Expression> = ir
            .select
            .iter()
            .filter_map(|ex| {
                if let Expression::Aggregate(Aggregate {
                    operator,
                    expression,
                }) = ex
                {
                    Some(ex.clone())
                } else {
                    None
                }
            })
            .collect();

        let output_refs: Vec<Expression> = ir
            .select
            .iter()
            .enumerate()
            .map(|exp| {
                if let Expression::Aggregate(Aggregate {
                    operator,
                    expression,
                }) = exp.1
                {
                    let idx = aggregates.element_offset(exp.1).unwrap();
                    Expression::ColumnIndex(ColumnIndex {
                        index: ir.groupby.len() + idx,
                    })
                } else {
                    let idx = ir.groupby.element_offset(exp.1).unwrap();
                    Expression::ColumnIndex(ColumnIndex { index: idx })
                }
            })
            .collect();

        if aggregates.len() == 0 {
            if ir.groupby.len() > 0 {
                bail!("GROUP BY without aggregate expressions is not supported")
            }

            return Ok(AnalyzedQuery {
                class: QueryClass::Relational,
                select: ir.select,
                wherec: ir.wherec,
                groupby: ir.groupby,
                having: ir.having,
                interim_projections: None,
                aggregates,
                outputReferences: None,
            });
        } else {
            // validate that the select expressions are present in the group by clause as well
            // because this is an aggregate query
            for item in ir.select.iter() {
                match item {
                    Expression::Aggregate(Aggregate {
                        operator,
                        expression,
                    }) => {}
                    _ => {
                        if !ir.groupby.contains(item) {
                            bail!("select expression must be present in groupby")
                        }
                    }
                }
            }

            Ok(AnalyzedQuery {
                class: QueryClass::Aggregate,
                select: ir.select,
                wherec: ir.wherec,
                groupby: ir.groupby,
                having: ir.having,
                interim_projections: None,
                aggregates,
                outputReferences: Some(output_refs),
            })
        }
    }

    /// parses the AST's from clause and returns the table name as
    /// an owned string
    fn parse_table_name(&self, ast: &Statement) -> Option<String> {
        let Statement::Query(q) = ast else {
            return None;
        };
        let SetExpr::Select(selast) = q.body.borrow() else {
            return None;
        };
        // Today we only support a single base table: SELECT ... FROM employee
        let table_with_joins = selast.from.first()?;
        let ast::TableFactor::Table { name, .. } = &table_with_joins.relation else {
            return None;
        };
        // ObjectName is a path like ["schema", "table"]; catalog keys are the
        // final identifier (e.g. "employee").
        let part = name.0.last()?;
        let ast::ObjectNamePart::Identifier(ident) = part else {
            return None;
        };
        Some(ident.value.clone())
    }

    fn query_ir(&self, ast: &Statement) -> Result<QueryIR> {
        let mut ir = QueryIR::new();

        match ast {
            Statement::Query(q) => {
                let SetExpr::Select(selast) = q.body.borrow() else {
                    bail!("select statement has no body")
                };

                // build projections
                for si in selast.projection.iter() {
                    match si {
                        SelectItem::UnnamedExpr(ex) => {
                            let cex = ex.convert();
                            if cex.is_some() {
                                ir.select.push(cex.unwrap());
                            }
                        }
                        SelectItem::ExprWithAlias { expr, alias } => {
                            let cex = expr.convert();
                            if cex.is_some() {
                                ir.select.push(cex.unwrap());
                            }
                        }
                        _ => unimplemented!(),
                    }
                }

                // -------------------

                // build selection/ filtering (where clause parsing)
                if let Some(parsed_where) = selast.selection.as_ref().and_then(|wh| wh.convert()) {
                    println!("where: {:?}", parsed_where);
                    ir.wherec = Some(parsed_where);
                }

                // -------------------

                // build group by expressioins
                match &selast.group_by {
                    GroupByExpr::All(_) => bail!("GROUP BY ALL syntax not supported"),
                    GroupByExpr::Expressions(exprs, _) => {
                        for ex in exprs.iter() {
                            if let Some(parsed_ex) = ex.convert() {
                                ir.groupby.push(parsed_ex);
                            }
                        }
                    }
                }

                // -------------------

                // build having expressioins
                if let Some(parsed_having) = selast.having.as_ref().and_then(|wh| wh.convert()) {
                    println!("having: {:?}", parsed_having);
                    ir.having = Some(parsed_having);
                }
            }

            _ => bail!("only select statements supported"),
        }
        Ok(ir)
    }

    /// takes a SQL AST and gives back a dataframe
    /// only supports SELECT queries today
    pub fn dataframe_from_sql(
        &self,
        ast: &Statement,
        table_catalog: &HashMap<String, DataFrame>,
    ) -> Result<DataFrame> {
        let Some(table_name) = self.parse_table_name(ast) else {
            bail!("query should have a FROM clause")
        };

        let ir = self.query_ir(ast)?;
        let ar = self.analyze(ir)?;

        // -------------- Query building logic here ------------
        match &ar.class {
            QueryClass::Relational => {
                let df = table_catalog.get(&table_name).and_then(|leaf_df| {
                    // the whole query building logic is built upon the primitive
                    // leaf dataframe which is created during datasource creation time
                    // only the leaf df has access to the underlying datasource the
                    // rest of the logical plan compute nodes are built on top of this
                    // and only contain child compute node metadata information
                    let mut final_df = leaf_df.clone();

                    // apply projection i.e. select expressions
                    // if we have interim projections we choose to apply those
                    // as they are a superset consisting of select + interim
                    // projected expressions
                    if let Some(interim) = ar.interim_projections {
                        final_df = final_df.project(interim);
                    }

                    // apply selection/ filter i.e. where expressions
                    if let Some(filter_expr) = ar.wherec {
                        final_df = final_df.filter(filter_expr);
                    }

                    // post a where clause, we add a check incase our projection
                    // list was widened by interim projections, we need to bring it
                    // back down to the actual select list
                    // this is because by now, the main select list columns are
                    // computed, so we need to keep the first N expressions from the
                    // original select clause
                    let l = ar.select.len();
                    let Some(fes) = final_df.schema().ok() else {
                        return None;
                    };

                    // the length of the select clause expressions list matches that
                    // of the expression list length produced by the schema of the final
                    // dataframe, so we dont need to de-flatten anything, simply return the
                    // this final produced dataframe
                    if ar.select.len() == fes.fields.len() {
                        return Some(final_df);
                    }

                    let mut deflattened_select_exprs = Vec::with_capacity(l);

                    // assertion/ invariant: the fields produced by a dataframe will either be equal to the select
                    // clause list or greater, so the for_each is infallible
                    ar.select.iter().enumerate().for_each(|expr| {
                        // acccording to above mentioned invariant this unwrap should always be safe
                        let corr_name = fes.fields.iter().nth(expr.0).unwrap();
                        let column_expr = Expression::Column(Column {
                            name: String::from(corr_name.name.clone()),
                        });
                        deflattened_select_exprs.push(column_expr);
                    });

                    final_df = final_df.project(deflattened_select_exprs);

                    Some(final_df)
                });

                if df.is_none() {
                    bail!("could not create Dataframe for relational class query");
                }

                Ok(df.unwrap())
            }
            QueryClass::Aggregate => {
                let df = table_catalog.get(&table_name).and_then(|leaf_df| {
                    let mut final_df = leaf_df.clone();
                    let mut hs = HashSet::new();
                    ar.select.iter().for_each(|e| {
                        let _ = hs.insert(e);
                    });
                    ar.aggregates.iter().for_each(|e| {
                        let _ = hs.insert(e);
                    });
                    ar.groupby.iter().for_each(|e| {
                        let _ = hs.insert(e);
                    });

                    let mut interim = Vec::new();
                    ar.groupby.iter().for_each(|exp| {
                        if !hs.contains(exp) {
                            interim.push(exp.clone());
                        }
                    });

                    final_df = final_df
                        .project(interim)
                        .filter(ar.wherec.unwrap())
                        .aggregate(ar.aggregates)
                        .project(ar.outputReferences.unwrap());

                    Some(final_df)
                });

                if df.is_none() {
                    bail!("could not create Dataframe for aggregate class query");
                }

                Ok(df.unwrap())
            }
        }
    }
}

trait Convert {
    fn convert(&self) -> Option<Expression>;
}

impl Convert for Expr {
    fn convert(&self) -> Option<Expression> {
        println!("test convert: {}", self);

        match self {
            // This block i.e. Expr::Value is mainly literal parsing logic,
            // small units of a larger Expression such as a string literal
            // in a Binary operator, etc.
            Expr::Value(v) => match &v.value {
                ast::Value::Number(num, ok) => {
                    let f = f64::from_str(num.as_str()).ok()?;
                    Some(Expression::LiteralDouble(LiteralDouble { value: f }))
                }
                // TODO: implement others
                ast::Value::SingleQuotedString(s) => {
                    Some(Expression::LiteralString(LiteralString {
                        value: s.clone(),
                    }))
                }
                _ => unimplemented!("ast::Value only supports Number"),
            },
            Expr::Nested(v) => v.convert(),
            Expr::Identifier(ident) => Some(Expression::Column(Column {
                name: ident.value.clone(),
            })),
            Expr::Function(f) => {
                // println!("func {:?}", f);
                let mut aggregate_name = String::new();
                let mut column_name = String::new();

                let item = f.name.0.iter().next()?;
                let ast::ObjectNamePart::Identifier(ident) = item else {
                    return None;
                };
                aggregate_name = ident.value.clone();

                let ast::FunctionArguments::List(l) = &f.args else {
                    return None;
                };
                for arg in l.args.iter() {
                    let ast::FunctionArg::Unnamed(unf) = arg else {
                        continue;
                    };
                    match unf {
                        ast::FunctionArgExpr::Expr(ex) => match ex {
                            ast::Expr::Identifier(ide) => {
                                column_name = ide.value.clone();
                            }
                            _ => unimplemented!(
                                "ast::FunctionArgExpr::Expr only supports identifier"
                            ),
                        },
                        ast::FunctionArgExpr::Wildcard => {
                            column_name = "*".to_string();
                        }
                        _ => unimplemented!(
                            "ast::FunctionArgExpr only supports expresison and wildcard"
                        ),
                    };
                }
                let Some(op) = Operator::from_str(aggregate_name.as_str()) else {
                    return None;
                };
                let aggr = Aggregate {
                    operator: op,
                    expression: Box::new(Expression::Column(Column { name: column_name })),
                };
                Some(Expression::Aggregate(aggr))
            }
            Expr::BinaryOp { left, op, right } => {
                let Some(l) = left.convert() else { return None };
                let Some(r) = right.convert() else {
                    return None;
                };
                let op = BinaryOperator::from_sql_binary_op(&op);
                Some(Expression::Binary(Binary {
                    operator: op,
                    L: Box::new(l),
                    R: Box::new(r),
                }))
            }
            _ => None,
        }
    }
}

/// A QueryIR contains converted AST SQL expression tree
/// to native Expression type. This is called as an Intermediate Representation
/// i.e an IR stage which we use to perform query analysis on. This involves
/// validating existence of columns against the provided Schema, interim projection
/// calculation usage and column indexes for post aggregation computations
struct QueryIR {
    select: Vec<Expression>,
    wherec: Option<Expression>,
    groupby: Vec<Expression>,
    having: Option<Expression>,
}

/// only supports select for now
impl QueryIR {
    fn new() -> Self {
        QueryIR {
            select: Vec::new(),
            groupby: Vec::new(),
            having: None,
            wherec: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // #[ignore = "changes made to code but yet to change this test"]
    #[test]
    fn test_dataframe_from_sql() -> Result<(), Box<dyn std::error::Error>> {
        use sqlparser::dialect::GenericDialect;
        use sqlparser::parser::Parser;
        let dialect = GenericDialect {};

        let sql = "SELECT
              state,
              first_name,
              salary - 100 AS adjusted,
              salary * 1.1 AS boosted,
              (salary + 500) / 12 AS monthly,
              SUM(salary) AS total_salary,
              MIN(salary) AS min_salary,
              MAX(salary) AS max_salary,
              AVG(salary) AS avg_salary,
              COUNT(*) AS row_count,
              COUNT(first_name) AS name_count
            FROM employee
            WHERE (
                state = 'CO'
                OR (state != 'TX' AND salary > 1000.0)
              )
              AND salary >= 500
              AND salary < 200000
              AND salary <= 150000
              AND id % 2 = 0
            GROUP BY
              state,
              first_name,
              salary - 100,
              salary * 1.1,
              (salary + 500) / 12;";

        let ast = Parser::parse_sql(&dialect, sql).unwrap();
        let mut tables: HashMap<String, DataFrame> = HashMap::new();
        let planner = Planner::new(tables);

        for elem in ast.iter() {
            let res = planner.query_ir(elem).unwrap();
            for item in res.select.iter() {
                println!("expressison DEBUG: {:?}", item);
            }
        }

        Ok(())
    }
}
