use crate::datasource;
use crate::datasource::Csv;
use crate::datasource::DataSource;
use crate::expressions;
use crate::expressions::*;
use crate::schema::*;
use anyhow::Ok;
use anyhow::Result;
use std::fmt;
use std::fmt::Display;
use std::rc::Rc;

/// A dataframe is a representation of a logical plan
/// a logical plan in turn is a tree structure where
/// each node represents the computation of a part of a query
/// against a particular schema
/// without specifying exactly how to execute it
/// This is also why an implementor of a dataframe is a
/// logical plan node such as an aggregate, scan, filter
/// and also why it gives a Schema against which that node
/// does the computation
///
/// DataFrames should always have a shorter lifetime than
/// datasources. This is because a data source is the source
/// of the truth, dataframes are ephemeral query execution plans
/// that are executed and forgotten about
pub struct DataFrame {
    plan: Rc<LogicalPlanNode>,
}

/// A cheap clone because we simply bump the reference-count
impl Clone for DataFrame {
    fn clone(&self) -> Self {
        DataFrame {
            plan: Rc::clone(&self.plan),
        }
    }
}

impl DataFrame {
    pub fn schema(&self) -> Result<Schema> {
        self.plan.schema()
    }

    /// Create a dataframe with an initial logical plan
    pub fn with(plan: LogicalPlanNode) -> Self {
        Self {
            plan: Rc::new(plan),
        }
    }

    pub fn logical_plan(self) -> Rc<LogicalPlanNode> {
        self.plan.clone()
    }

    /// creates a projection logical-plan node against the given expressions
    /// and the schema of the node
    /// consumes `self` and returns a new DataFrame object
    /// the ownership of the current self.plan is transferred to the new parent
    /// logicalplan node we build in this function on top of the current self.plan
    /// logicalplan node
    pub fn project(self, expressions: Vec<Expression>) -> DataFrame {
        DataFrame {
            plan: Rc::new(LogicalPlanNode::ProjectNode(Box::new(Projection {
                input: self.plan.clone(),
                exprs: expressions,
            }))),
        }
    }

    pub fn filter(self, expression: Expression) -> DataFrame {
        DataFrame {
            plan: Rc::new(LogicalPlanNode::FilterNode(Box::new(Filter {
                input: self.plan.clone(),
                exprs: expression,
            }))),
        }
    }

    pub fn join(self, right: DataFrame, join_type: JoinType, on: Vec<JoinKey>) -> DataFrame {
        DataFrame {
            plan: Rc::new(LogicalPlanNode::JoinNode(Box::new(Join {
                left: self.plan.clone(),
                right: right.logical_plan(),
                join_type,
                on,
            }))),
        }
    }

    pub fn aggregate(self, expressions: Vec<Expression>) -> DataFrame {
        DataFrame {
            plan: Rc::new(LogicalPlanNode::AggregateNode(Box::new(Aggregation {
                input: self.plan.clone(),
                exprs: expressions,
            }))),
        }
    }
}

/// A computation node in the tree of dataframe.
/// a dataframe is a sequence of LogicalPlanNode's where each node
/// specifies what the computation is. Following are the types of
/// computations that implement this trait -
/// - Projection
/// - Filter/ Selection
/// - Aggregation
/// - Join
/// - Scan (this is always the first computation i.e. the leaf node in the computation tree)
impl LogicalPlanNode {
    /// schema gives back a Schema type with fields that are converted from the expressions that
    /// are specific to this computation node. The table level schema is the larger picture schema
    /// of the entire dataset, whereas the Schema specific to this computation node is specific to it
    /// and hence might contain a subset of fields required for the said computation.
    /// Infallible
    fn schema(&self) -> Result<Schema> {
        match self {
            LogicalPlanNode::AggregateNode(n) => {
                let mut fields = Vec::new();
                for ex in n.exprs.iter() {
                    let af = ex.to_field()?;
                    fields.push(af);
                }
                Ok(Schema { fields })
            }
            LogicalPlanNode::ProjectNode(n) => {
                let mut fields = Vec::new();
                for ex in n.exprs.iter() {
                    let af = ex.to_field()?;
                    fields.push(af);
                }
                Ok(Schema { fields })
            }
            LogicalPlanNode::JoinNode(n) => {
                let mut fields = Vec::new();
                let mut left = n.left.schema()?.fields;
                let mut right = n.right.schema()?.fields;
                match n.join_type {
                    JoinType::Left => {
                        fields.append(&mut left);
                        let newr: Vec<Field> = right
                            .iter_mut()
                            .filter_map(|f| {
                                if !left.contains(f) {
                                    Some(f.to_owned())
                                } else {
                                    None
                                }
                            })
                            .collect();
                        Ok(Schema { fields })
                    }
                    _ => unimplemented!(),
                }
            }
            _ => return self.schema(),
        }
    }

    /// children gives back the list of Dataframe trait objects that are inputs to this logical plan
    /// compute node. The computation tree is a bottom-up order, meaning the computation of a query
    /// always starts from a Scan node (since it's the leaf node) and then that node is the input to
    /// it's parent node and so on and so forth until the root node which describes the entire logical
    /// plan computation tree.
    fn children(&self) -> Option<&[LogicalPlanNode]> {
        unimplemented!()
    }
}

pub enum LogicalPlanNode {
    ProjectNode(Box<Projection>),
    FilterNode(Box<Filter>),
    AggregateNode(Box<Aggregation>),
    JoinNode(Box<Join>),
    ScanNode(Box<Scan>),
}

pub struct Projection {
    input: Rc<LogicalPlanNode>,
    exprs: Vec<Expression>,
}

pub struct Filter {
    input: Rc<LogicalPlanNode>,
    exprs: Expression,
}

pub struct Aggregation {
    input: Rc<LogicalPlanNode>,
    exprs: Vec<Expression>,
}

pub enum JoinType {
    Inner,
    Left,
    Right,
}

pub struct Join {
    join_type: JoinType,
    left: Rc<LogicalPlanNode>,
    right: Rc<LogicalPlanNode>,
    on: Vec<JoinKey>,
}

#[derive(Debug)]
pub struct JoinKey {
    left: String,
    right: String,
}

pub enum ScanSource {
    Csv(Csv),
}

pub struct Scan {
    pub datasource: ScanSource,
}

impl fmt::Display for LogicalPlanNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogicalPlanNode::ProjectNode(p) => {
                let exprs: Vec<String> = p.exprs.iter().map(|e| e.to_string()).collect();
                write!(f, "Projection: {}", exprs.join(", "))
            }
            LogicalPlanNode::FilterNode(filter) => {
                write!(f, "Selection: {}", filter.exprs)
            }
            LogicalPlanNode::AggregateNode(a) => {
                // gquery splits groupExpr / aggregateExpr; until you do too, print what you have:
                let exprs: Vec<String> = a.exprs.iter().map(|e| e.to_string()).collect();
                write!(f, "Aggregate: aggregateExpr=[{}]", exprs.join(", "))
                // later:
                // write!(f, "Aggregate: groupExpr=[{}], aggregateExpr=[{}]", ...)
            }
            LogicalPlanNode::JoinNode(j) => {
                write!(f, "Join: type={}, on={:?}", j.join_type, j.on)
            }
            LogicalPlanNode::ScanNode(s) => match &s.datasource {
                ScanSource::Csv(csv) => {
                    // expose path (+ optional projection) on Csv, then:
                    write!(f, "Scan: {}; projection=None", csv.path)
                    // or: write!(f, "Scan: {}; projection={:?}", csv.path, csv.projections)
                }
            },
        }
    }
}

impl fmt::Display for JoinType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JoinType::Inner => write!(f, "Inner"),
            JoinType::Left => write!(f, "Left"),
            JoinType::Right => write!(f, "Right"),
        }
    }
}

fn format_plan(plan: &LogicalPlanNode, indent: usize) -> String {
    let pad = "\t".repeat(indent);
    let mut out = format!("{}{}\n", pad, plan);

    match plan {
        LogicalPlanNode::ProjectNode(p) => {
            out.push_str(&format_plan(&p.input, indent + 1));
        }
        LogicalPlanNode::FilterNode(p) => {
            out.push_str(&format_plan(&p.input, indent + 1));
        }
        LogicalPlanNode::AggregateNode(p) => {
            out.push_str(&format_plan(&p.input, indent + 1));
        }
        LogicalPlanNode::JoinNode(j) => {
            out.push_str(&format_plan(&j.left, indent + 1));
            out.push_str(&format_plan(&j.right, indent + 1));
        }
        LogicalPlanNode::ScanNode(_) => {}
    }

    out
}

impl fmt::Display for DataFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", format_plan(&self.plan, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dataframe_basic_usage() {
        let f = Field {
            name: String::from("name"),
            datatype: arrow::datatypes::DataType::Utf8,
        };

        let mut fs = Vec::new();
        fs.push(f);

        let sc = Schema { fields: fs };

        let pa = String::from("employees.csv");
        let pros = vec!["s1", "s2"];

        let scan = Scan {
            datasource: ScanSource::Csv(
                Csv::new(&pa, pros.as_slice(), 3, Rc::new(sc))
                    .expect("failed to create csv datasource"),
            ),
        };

        let lf = LogicalPlanNode::ScanNode(Box::new(scan));
        let df = DataFrame::with(lf);

        // --------------

        let mut agg_exps = Vec::new();
        agg_exps.push(Expression::Aggregate(Aggregate {
            operator: Operator::SUM,
            expression: Box::new(Expression::Column(Column {
                name: String::from("salary"),
            })),
        }));

        let mut filter_exps = Expression::Column(Column {
            name: String::from("age"),
        });

        let dataframe = df.aggregate(agg_exps).filter(filter_exps);
    }

    #[test]
    fn scan_node_basic() {
        let f = Field {
            name: String::from("name"),
            datatype: arrow::datatypes::DataType::Utf8,
        };

        let mut fs = Vec::new();
        fs.push(f);

        let sc = Schema { fields: fs };

        let pa = String::from("employees.csv");
        let pros = vec!["s1", "s2"];

        let scan = Scan {
            datasource: ScanSource::Csv(
                Csv::new(&pa, pros.as_slice(), 3, Rc::new(sc))
                    .expect("failed to create csv datasource"),
            ),
        };
    }

    #[test]
    fn sql_agg() {
        use sqlparser::dialect::GenericDialect;
        use sqlparser::parser::Parser;
        let dialect = GenericDialect {}; // or AnsiDialect

        let sql = "SELECT a, SUM(b) FROM table_1";

        let ast = Parser::parse_sql(&dialect, sql).unwrap();

        for elem in ast.iter() {
            println!("AST: {:?}", elem);
        }
    }
}
