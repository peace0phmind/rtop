use crate::{Binding, RdfTerm, RuntimeError};
use chrono::{DateTime, NaiveDateTime};
use regex::Regex;
use std::collections::BTreeMap;

const RDF_TYPE: &str = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";

#[derive(Debug, Clone)]
pub struct TriplePattern {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    /// `None` 表示默认图；具名图只适用于 facts，不会下推到关系数据源 mapping。
    pub graph: Option<String>,
}

#[derive(Debug, Default)]
struct DatasetClauses {
    default_graphs: Vec<String>,
    named_graphs: Vec<String>,
    declared: bool,
}

#[derive(Debug, Clone)]
pub enum Query {
    Select {
        variables: Vec<String>,
        pattern: GraphPattern,
        aggregates: Vec<Aggregate>,
        projection_binds: Vec<Bind>,
        group_by: Vec<GroupBy>,
        having: Vec<Expression>,
        distinct: bool,
        order_by: Vec<OrderByTerm>,
        offset: Option<usize>,
        limit: Option<usize>,
    },
    Ask {
        pattern: GraphPattern,
    },
    Construct {
        template: Vec<TriplePattern>,
        /// CONSTRUCT 的 WHERE 可包含子查询、FILTER、OPTIONAL 等完整 graph pattern，
        /// 不能收窄成 BGP triples。
        pattern: GraphPattern,
    },
    Describe {
        /// DESCRIBE 接受多个固定 IRI 或由 WHERE pattern 绑定的变量。
        resources: Vec<String>,
        pattern: Option<GraphPattern>,
    },
}

/// parser 到 runtime 的查询代数 seam。
#[derive(Debug, Clone)]
pub enum GraphPattern {
    Empty,
    Bgp(Vec<TriplePattern>),
    /// 单一谓词的零或多次 property path。运行时在已选出的有限 RDF 图上求闭包，
    /// 不能降为 BGP，因为零长度与环路都不是 SQL JOIN 可以表达的语义。
    ZeroOrMorePath(TriplePattern),
    /// IRI sequence 的零或多次 property path。一次关系先按整个 sequence 求值，
    /// 随后才做集合闭包；不能把 sequence 展开后的 BGP 结果直接当作闭包边。
    ZeroOrMoreSequencePath {
        pattern: TriplePattern,
        predicates: Vec<String>,
    },
    /// 精确 `{0}` property path：只产生同一 RDF term 的 identity mapping，
    /// 不要求图中存在该 predicate 的边。
    ZeroLengthPath(TriplePattern),
    Join(Box<GraphPattern>, Box<GraphPattern>),
    LeftJoin(Box<GraphPattern>, Box<GraphPattern>),
    Minus(Box<GraphPattern>, Box<GraphPattern>),
    Union(Box<GraphPattern>, Box<GraphPattern>),
    Subquery {
        pattern: Box<GraphPattern>,
        variables: Vec<String>,
        aggregates: Vec<Aggregate>,
        projection_binds: Vec<Bind>,
        group_by: Vec<GroupBy>,
        having: Vec<Expression>,
        distinct: bool,
        order_by: Vec<OrderByTerm>,
        offset: Option<usize>,
        limit: Option<usize>,
    },
    /// VALUES 的每项是一条 solution mapping；单变量和 tuple 形式共用同一表示。
    Values(Vec<Binding>),
    Bind(Box<GraphPattern>, Vec<Bind>),
    /// dataset 展开为多个 GRAPH 分支时保留可观察的 graph variable binding；它不
    /// 是用户 BIND alias，因而不能被 UNION 的 alias-scope 清理逻辑移除。
    DatasetGraphBind {
        pattern: Box<GraphPattern>,
        variable: String,
        graph: String,
    },
    /// 花括号嵌套 group 不会把外层 BIND 引入的变量带入内部 lexical scope。
    /// 执行时仅在内部模式求值前遮蔽这些变量，结果仍与外层 mapping 正常 join。
    Scoped {
        pattern: Box<GraphPattern>,
        hidden: Vec<String>,
    },
    Filter(Box<GraphPattern>, Vec<Filter>),
    /// `FILTER EXISTS` / `FILTER NOT EXISTS` 的右侧必须以当前 outer binding 关联求值。
    Exists {
        pattern: Box<GraphPattern>,
        exists: Box<GraphPattern>,
        negated: bool,
    },
    /// `FILTER(expression || [NOT] EXISTS { ... })` 保持同一 outer solution
    /// 的短路布尔语义，不能降为 UNION，否则两个分支同时为真时会重复结果。
    FilterOrExists {
        pattern: Box<GraphPattern>,
        filter: Filter,
        exists: Box<GraphPattern>,
        negated: bool,
    },
}

/// `FROM` 的默认图是所列图的 RDF merge，不能把 property-path 闭包按图分开后
/// 再 UNION。该内部标记仅在 parser/runtime seam 间传递，用户 IRI 不可构造它。
const DATASET_DEFAULT_PATH_GRAPHS: &str = "__rtop_dataset_default_path_graphs__";

/// `BIND (expression AS ?variable)` 的最小稳定表示。表达式会在图模式匹配之后、
/// FILTER 之前求值；未能求值的行保留但不绑定目标变量（SPARQL expression error）。
#[derive(Debug, Clone)]
pub struct Bind {
    pub variable: String,
    pub expression: Expression,
}

/// SELECT 投影中的聚合，在完整 graph pattern 的 solution sequence 上执行。
#[derive(Debug, Clone)]
pub struct Aggregate {
    pub variable: String,
    pub kind: AggregateKind,
    pub expression: Expression,
    /// COUNT(*) 计数每条 solution mapping，不要求任一 expression binding。
    pub count_all: bool,
    pub distinct: bool,
    pub separator: Option<String>,
    /// 聚合 expression error 时由 SELECT 投影的 COALESCE 使用的后备表达式。
    pub fallback: Option<Expression>,
}

/// GROUP BY 的变量或表达式别名。表达式必须在形成 group key 前求值，不能误作
/// SELECT projection bind，否则 aggregate 会先看到未分组的 solution sequence。
#[derive(Debug, Clone)]
pub enum GroupBy {
    Variable(String),
    Expression {
        expression: Expression,
        variable: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum AggregateKind {
    Count,
    Sum,
    Avg,
    Min,
    Max,
    Sample,
    GroupConcat,
}

#[derive(Debug, Clone)]
pub enum Expression {
    Not(Box<Expression>),
    Variable(String),
    String(String),
    LanguageLiteral {
        value: String,
        language: String,
    },
    TypedLiteral {
        value: String,
        datatype: String,
    },
    Iri(String),
    Number(String),
    Aggregate {
        kind: AggregateKind,
        expression: Box<Expression>,
        count_all: bool,
        distinct: bool,
    },
    Binary {
        operator: ArithmeticOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Logical {
        operator: LogicalOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Comparison {
        operator: ComparisonOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    In {
        value: Box<Expression>,
        candidates: Vec<Expression>,
        negated: bool,
    },
    Function {
        name: String,
        arguments: Vec<Expression>,
        /// 查询开头的 BASE，只由 IRI()/URI() 在运行时使用。
        base_iri: Option<String>,
    },
    Replace {
        value: Box<Expression>,
        pattern: Box<Expression>,
        replacement: Box<Expression>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum ArithmeticOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

#[derive(Debug, Clone, Copy)]
pub enum LogicalOperator {
    And,
    Or,
}

#[derive(Debug, Clone, Copy)]
pub enum ComparisonOperator {
    Equal,
    NotEqual,
    Less,
    Greater,
    LessOrEqual,
    GreaterOrEqual,
}

#[derive(Debug, Clone)]
pub enum Filter {
    Language {
        variable: String,
        language: String,
    },
    Equal {
        variable: String,
        value: FilterValue,
    },
    Expression(Expression),
}

#[derive(Debug, Clone)]
pub enum FilterValue {
    Literal {
        value: String,
        datatype: Option<String>,
    },
    Numeric(f64),
}

#[derive(Debug, Clone)]
pub struct OrderByTerm {
    pub expression: Expression,
    pub descending: bool,
}

pub fn parse(input: &str) -> Result<Query, RuntimeError> {
    // 固定 Ontop manifest 中既有 CRLF 的 .rq 文件；在词法解析前归一化行结束符，
    // 保持 PREFIX、注释和 token 边界与 LF 输入一致。
    let input = input.replace("\r\n", "\n").replace('\r', "\n");
    let input = strip_sparql_comments(&input);
    // 当前 lexer 的 literal 表示统一使用双引号。SPARQL 的 long-string
    // delimiter `'''` / `\"\"\"` 仅改变词法边界、不改变 value；先归一化它们，
    // 使后续 PREFIX、变量和 token 处理能把多行内容完整视为一个 literal。
    // 固定 DAWG basic/quotes 资产没有 delimiter 内嵌同型三连引号。
    let input = normalize_long_string_delimiters(&input);
    let input = normalize_single_quoted_literals(&input);
    // DAWG syntax-query 保留历史 BINDINGS spelling；固定 Ontop 基线将其作为
    // VALUES 的兼容语法接受，故归一到唯一的 VALUES parser 路径。
    let input = input.replace("BINDINGS", "VALUES");
    // SPARQL 1.1 将 `$name` 与 `?name` 定义为同一变量语法。先在词法层归一化，
    // 让投影、BGP、FILTER、BIND、GROUP BY 和 ORDER BY 共用既有的 `?` 解析路径，
    // 同时不改写 IRI 或 string literal 内的 `$`。
    let input = normalize_dollar_variables(&input);
    let (input, base_iri) = extract_base(&input)?;
    let input = expand_prefixes(&input)?;
    let input = normalize_bare_boolean_literals(&input);
    validate_typed_boolean_literals(&input)?;
    validate_typed_datetime_literals(&input)?;
    validate_blank_node_term_positions(&input)?;
    let input = expand_blank_node_property_lists(&input);
    let input = expand_collections(&input)?;
    let input = expand_anonymous_blank_nodes(&input);
    let compact = input.trim();
    let upper = compact.to_ascii_uppercase();
    // DAWG functions 的 SELECT 投影可以换行开始；SELECT 后接受任意 SPARQL
    // whitespace，而不是只接受一个 ASCII space。
    if upper.starts_with("SELECT")
        && compact
            .as_bytes()
            .get("SELECT".len())
            .is_some_and(u8::is_ascii_whitespace)
    {
        let open = compact
            .find('{')
            .ok_or_else(|| RuntimeError::MalformedSparql("缺少 `{`".into()))?;
        let projection = compact[6..open].trim();
        // WHERE 在 SELECT projection 与 group pattern 间是可选关键字；固定 DAWG
        // .rq 同时含有省略和显式两种写法，不能把显式 WHERE 当作投影变量。
        let projection = projection
            .to_ascii_uppercase()
            .ends_with("WHERE")
            .then(|| projection[..projection.len() - "WHERE".len()].trim_end())
            .unwrap_or(projection);
        let (projection, mut dataset) = parse_dataset_clauses(projection)?;
        resolve_dataset_iris(&mut dataset, base_iri.as_deref());
        let (selected, mut binds, aggregates, distinct) = parse_select_projection(&projection)?;
        // WHERE group 后可以有 SPARQL 1.1 `VALUES` clause；不能使用最后一个 `}`，
        // 否则会把 VALUES 的数据块误当成 WHERE 的闭合括号。
        let close = matching_brace(compact, open)?;
        let mut pattern =
            apply_dataset_clauses(parse_graph_pattern(&compact[open..=close])?, &dataset);
        let pattern_variables = graph_pattern_variables(&pattern)
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        if let Some(variable) = binds
            .iter()
            .map(|bind| bind.variable.as_str())
            .chain(
                aggregates
                    .iter()
                    .map(|aggregate| aggregate.variable.as_str()),
            )
            .find(|variable| pattern_variables.contains(*variable))
        {
            return Err(RuntimeError::MalformedSparql(format!(
                "SELECT projection alias ?{variable} 已在 WHERE 作用域中绑定"
            )));
        }
        set_pattern_base_iri(&mut pattern, base_iri.as_deref());
        for bind in &mut binds {
            set_expression_base_iri(&mut bind.expression, base_iri.as_deref());
        }
        let mut tail = compact[close + 1..].trim();
        while starts_keyword_at(tail, 0, "VALUES") {
            let (values, next) = parse_values_at(tail, 0)?;
            pattern = join_graph_patterns(pattern, GraphPattern::Values(values));
            tail = tail[next..].trim_start();
        }
        let (group_by, having, order_by, offset, limit) = parse_modifiers(tail)?;
        // SPARQL 1.1 不允许在 GROUP BY 查询中使用 SELECT *；否则投影集合会
        // 依赖实现细节而不是显式的 group variable。DAWG syn-bad-01 明确规定
        // 这必须是 NegativeSyntaxTest11。
        if selected.is_none() && !group_by.is_empty() {
            return Err(RuntimeError::MalformedSparql(
                "GROUP BY 查询不允许 SELECT *".into(),
            ));
        }
        let select_star = selected.is_none();
        let variables = if select_star {
            graph_pattern_variables(&pattern)
                .into_iter()
                .chain(binds.iter().map(|bind| bind.variable.clone()))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect()
        } else {
            selected.expect("non-* projection parsed into variables")
        };
        // `SELECT *` 在图模式没有变量时是合法的，并投影空 solution mapping
        // （DAWG syn-pname-03）；只有显式但空的 projection 才是语法错误。
        if variables.is_empty() && !select_star {
            return Err(RuntimeError::MalformedSparql("SELECT 需要一个变量".into()));
        }
        validate_aggregate_projection(&variables, &binds, &aggregates, &group_by)?;
        return Ok(Query::Select {
            variables,
            pattern,
            aggregates,
            projection_binds: binds,
            group_by,
            having,
            distinct,
            order_by,
            offset,
            limit,
        });
    }
    if upper.starts_with("ASK") {
        let open = compact
            .find('{')
            .ok_or_else(|| RuntimeError::MalformedSparql("缺少 `{`".into()))?;
        let mut pattern = parse_graph_pattern(&compact[open..])?;
        set_pattern_base_iri(&mut pattern, base_iri.as_deref());
        if graph_pattern_has_predicate_variable(&pattern) {
            return Err(RuntimeError::NotFullyTranslatable(
                "ASK 尚不支持谓词变量".into(),
            ));
        }
        return Ok(Query::Ask { pattern });
    }
    if upper == "DESCRIBE" || upper.starts_with("DESCRIBE ") {
        let describe_tail = compact[8..].trim();
        let where_at = keyword_position(describe_tail, "WHERE");
        let (resources, pattern) = match where_at {
            Some(where_at) => (
                describe_tail[..where_at].trim(),
                Some(parse_graph_pattern(
                    describe_tail[where_at + "WHERE".len()..].trim(),
                )?),
            ),
            None => (describe_tail, None),
        };
        let resources = tokens(resources)?
            .into_iter()
            .map(|resource| {
                if resource.starts_with('<') && resource.ends_with('>') {
                    Ok(resource.trim_matches(['<', '>']).to_owned())
                } else if resource.starts_with('?') {
                    Ok(resource.to_owned())
                } else {
                    Err(RuntimeError::MalformedSparql(
                        "DESCRIBE target 必须是 IRI 或变量".into(),
                    ))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !resources.is_empty() {
            return Ok(Query::Describe { resources, pattern });
        }
        return Err(RuntimeError::MalformedSparql("DESCRIBE 缺少 target".into()));
    }
    if upper.starts_with("CONSTRUCT")
        && compact
            .as_bytes()
            .get("CONSTRUCT".len())
            .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'{')
    {
        let construct_tail = compact["CONSTRUCT".len()..].trim_start();
        let construct_upper = construct_tail.to_ascii_uppercase();
        // SPARQL 1.1 的 CONSTRUCT WHERE 简写将 WHERE graph pattern 同时作为
        // template；可在其间带 DatasetClause（DAWG syntax-construct-where-01/02）。
        if construct_upper.starts_with("WHERE")
            && construct_tail
                .as_bytes()
                .get("WHERE".len())
                .is_some_and(u8::is_ascii_whitespace)
        {
            let shorthand = construct_tail["WHERE".len()..].trim_start();
            // `CONSTRUCT WHERE { ... }` 的 shortcut 把括号中的内容同时用作
            // template 与 WHERE；template 只能包含 triples，不能含 FILTER 或 GRAPH
            // group graph pattern（SPARQL 1.1 constructwhere05/06）。
            if keyword_position(shorthand, "FILTER").is_some()
                || keyword_position(shorthand, "GRAPH").is_some()
            {
                return Err(RuntimeError::MalformedSparql(
                    "CONSTRUCT WHERE shortcut 的 template 仅支持三元组".into(),
                ));
            }
            let patterns = group(shorthand)?;
            return Ok(Query::Construct {
                template: patterns.clone(),
                pattern: GraphPattern::Bgp(patterns),
            });
        }
        let where_at = keyword_position(construct_tail, "WHERE")
            .ok_or_else(|| RuntimeError::MalformedSparql("CONSTRUCT 缺少 WHERE".into()))?;
        if construct_tail[..where_at]
            .trim_start()
            .to_ascii_uppercase()
            .starts_with("FROM")
        {
            let patterns = group(construct_tail[where_at + "WHERE".len()..].trim_start())?;
            return Ok(Query::Construct {
                template: patterns.clone(),
                pattern: GraphPattern::Bgp(patterns),
            });
        }
        let template = group(construct_tail[..where_at].trim())?;
        return Ok(Query::Construct {
            template,
            pattern: parse_graph_pattern(construct_tail[where_at + "WHERE".len()..].trim())?,
        });
    }
    Err(RuntimeError::UnsupportedSparql(
        "仅支持 SELECT、ASK、CONSTRUCT 与 DESCRIBE".into(),
    ))
}

/// `_:label` 与 `[]` 只能表达 triple 的 subject/object，不能在 predicate、
/// FILTER expression 或 GRAPH name 位置出现。必须在 `[]` 展开为内部变量前
/// 检查，否则会丢失原始的 blank-node token 类别。
fn validate_blank_node_term_positions(input: &str) -> Result<(), RuntimeError> {
    let graph_blank = Regex::new(r"(?i)\bGRAPH\s+(?:\[\]|_:[A-Za-z_][A-Za-z0-9_]*)")
        .expect("固定 GRAPH blank-node 正则有效");
    let predicate_blank = Regex::new(
        r#"(?s)(?:\?[A-Za-z_][A-Za-z0-9_]*|<[^>]+>|_:[A-Za-z_][A-Za-z0-9_]*)\s+(?:\[\]|_:[A-Za-z_][A-Za-z0-9_]*)\s+(?:\?|<|_:\w|\[|"|[-+]?[0-9])"#,
    )
    .expect("固定 predicate blank-node 正则有效");
    let filter_blank = Regex::new(r"(?i)\bFILTER\s*\(\s*_:[A-Za-z_][A-Za-z0-9_]*")
        .expect("固定 FILTER blank-node 正则有效");
    if graph_blank.is_match(input)
        || predicate_blank.is_match(input)
        || filter_blank.is_match(input)
    {
        return Err(RuntimeError::MalformedSparql(
            "blank node 不能作为 GRAPH、predicate 或 FILTER term".into(),
        ));
    }
    Ok(())
}

/// SPARQL `#` 注释可位于 SELECT projection 的行尾；仅在字符串和 IRI 之外将其
/// 剥离，避免破坏 `<...#fragment>` 或 `"#"` literal。
fn strip_sparql_comments(input: &str) -> String {
    input
        .lines()
        .map(|line| {
            let mut quoted = None;
            let mut escaped = false;
            let mut iri = false;
            for (index, character) in line.char_indices() {
                match character {
                    '\'' | '"' if !escaped && !iri => {
                        quoted = (quoted != Some(character)).then_some(character);
                    }
                    '<' if quoted.is_none() => iri = true,
                    '>' if iri => iri = false,
                    '#' if quoted.is_none() && !iri => return &line[..index],
                    _ => {}
                }
                escaped = character == '\\' && !escaped;
                if character != '\\' {
                    escaped = false;
                }
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn validate_aggregate_projection(
    variables: &[String],
    projection_binds: &[Bind],
    aggregates: &[Aggregate],
    group_by: &[GroupBy],
) -> Result<(), RuntimeError> {
    // 即使没有 aggregate，GROUP BY 也把未分组的 projection variable 排除在
    // 有效作用域之外（DAWG syn-bad-02）。无 GROUP BY、无 aggregate 的普通
    // SELECT 则不需要这层约束。
    if aggregates.is_empty() && group_by.is_empty() {
        return Ok(());
    }
    let group_variables = group_by
        .iter()
        .map(|group| match group {
            GroupBy::Variable(variable) => variable.as_str(),
            GroupBy::Expression { variable, .. } => variable.as_str(),
        })
        .collect::<std::collections::BTreeSet<_>>();
    let mut aggregate_variables = aggregates
        .iter()
        .map(|aggregate| aggregate.variable.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    aggregate_variables.extend(
        projection_binds
            .iter()
            .filter(|bind| expression_contains_aggregate(&bind.expression))
            .map(|bind| bind.variable.as_str()),
    );
    if let Some(variable) = variables.iter().find(|variable| {
        !group_variables.contains(variable.as_str())
            && !aggregate_variables.contains(variable.as_str())
    }) {
        return Err(RuntimeError::MalformedSparql(format!(
            "aggregate SELECT 中的 ?{variable} 必须出现在 GROUP BY"
        )));
    }
    for bind in projection_binds {
        if expression_contains_aggregate(&bind.expression) {
            continue;
        }
        if let Some(variable) = expression_variables(&bind.expression)
            .into_iter()
            .find(|variable| !group_variables.contains(variable))
        {
            return Err(RuntimeError::MalformedSparql(format!(
                "aggregate projection expression 使用了未分组变量 ?{variable}"
            )));
        }
    }
    Ok(())
}

fn expression_contains_aggregate(expression: &Expression) -> bool {
    match expression {
        Expression::Aggregate { .. } => true,
        Expression::Not(inner) => expression_contains_aggregate(inner),
        Expression::Binary { left, right, .. }
        | Expression::Logical { left, right, .. }
        | Expression::Comparison { left, right, .. } => {
            expression_contains_aggregate(left) || expression_contains_aggregate(right)
        }
        Expression::In {
            value, candidates, ..
        } => {
            expression_contains_aggregate(value)
                || candidates.iter().any(expression_contains_aggregate)
        }
        Expression::Function { arguments, .. } => {
            arguments.iter().any(expression_contains_aggregate)
        }
        Expression::Replace {
            value,
            pattern,
            replacement,
        } => {
            expression_contains_aggregate(value)
                || expression_contains_aggregate(pattern)
                || expression_contains_aggregate(replacement)
        }
        _ => false,
    }
}

fn expression_variables(expression: &Expression) -> Vec<&str> {
    match expression {
        Expression::Variable(variable) => vec![variable],
        Expression::Not(inner) => expression_variables(inner),
        Expression::Binary { left, right, .. }
        | Expression::Logical { left, right, .. }
        | Expression::Comparison { left, right, .. } => {
            let mut variables = expression_variables(left);
            variables.extend(expression_variables(right));
            variables
        }
        Expression::In {
            value, candidates, ..
        } => {
            let mut variables = expression_variables(value);
            for candidate in candidates {
                variables.extend(expression_variables(candidate));
            }
            variables
        }
        Expression::Function { arguments, .. } => {
            arguments.iter().flat_map(expression_variables).collect()
        }
        Expression::Replace {
            value,
            pattern,
            replacement,
        } => {
            let mut variables = expression_variables(value);
            variables.extend(expression_variables(pattern));
            variables.extend(expression_variables(replacement));
            variables
        }
        Expression::Aggregate { expression, .. } => expression_variables(expression),
        _ => Vec::new(),
    }
}

fn parse_dataset_clauses(input: &str) -> Result<(String, DatasetClauses), RuntimeError> {
    let matcher =
        Regex::new(r"(?i)\s+FROM\s+(?:(NAMED)\s+)?<([^>]+)>").expect("dataset clause 正则固定有效");
    let mut dataset = DatasetClauses::default();
    let mut projection = String::with_capacity(input.len());
    let mut end = 0usize;
    for captures in matcher.captures_iter(input) {
        let matched = captures.get(0).expect("dataset clause match");
        projection.push_str(&input[end..matched.start()]);
        let iri = captures.get(2).expect("dataset IRI").as_str().to_owned();
        if captures.get(1).is_some() {
            dataset.named_graphs.push(iri);
        } else {
            dataset.default_graphs.push(iri);
        }
        dataset.declared = true;
        end = matched.end();
    }
    projection.push_str(&input[end..]);
    Ok((projection, dataset))
}

fn resolve_dataset_iris(dataset: &mut DatasetClauses, base_iri: Option<&str>) {
    let Some(base_iri) = base_iri else {
        return;
    };
    let base = oxiri::Iri::parse(base_iri.to_owned()).expect("BASE 已在解析时验证");
    for graph in dataset
        .default_graphs
        .iter_mut()
        .chain(dataset.named_graphs.iter_mut())
    {
        if oxiri::Iri::parse(graph.clone()).is_err() {
            if let Ok(resolved) = oxiri::IriRef::from(base.clone()).resolve(graph.as_str()) {
                *graph = resolved.into_inner();
            }
        }
    }
}

/// 将显式 dataset 施加到基本图模式。每一个未限定图的 triple 都从 FROM 的 RDF
/// merge 中读取；这里保留为各图 BGP 的 UNION，因而自然保持 SPARQL 的 bag join。
/// FROM NAMED 仅限定 GRAPH IRI 的可见集合，不会泄漏到默认图。
fn apply_dataset_clauses(pattern: GraphPattern, dataset: &DatasetClauses) -> GraphPattern {
    match pattern {
        GraphPattern::Bgp(patterns) => {
            patterns
                .into_iter()
                .fold(GraphPattern::Empty, |current, triple| {
                    let item = match triple.graph.as_deref() {
                        None if dataset.declared && dataset.default_graphs.is_empty() => {
                            GraphPattern::Values(Vec::new())
                        }
                        None if dataset.default_graphs.is_empty() => {
                            GraphPattern::Bgp(vec![triple])
                        }
                        None => dataset
                            .default_graphs
                            .iter()
                            .cloned()
                            .map(|graph| {
                                let mut triple = triple.clone();
                                triple.graph = Some(graph);
                                GraphPattern::Bgp(vec![triple])
                            })
                            .reduce(|left, right| {
                                GraphPattern::Union(Box::new(left), Box::new(right))
                            })
                            .expect("non-empty dataset default graphs"),
                        Some(graph) if dataset.declared && graph.starts_with('?') => dataset
                            .named_graphs
                            .iter()
                            .cloned()
                            .map(|named| {
                                let mut triple = triple.clone();
                                triple.graph = Some(named.clone());
                                GraphPattern::DatasetGraphBind {
                                    pattern: Box::new(GraphPattern::Bgp(vec![triple])),
                                    variable: graph.trim_start_matches('?').into(),
                                    graph: named,
                                }
                            })
                            .reduce(|left, right| {
                                GraphPattern::Union(Box::new(left), Box::new(right))
                            })
                            .unwrap_or_else(|| GraphPattern::Values(Vec::new())),
                        Some(graph)
                            if dataset.declared
                                && !dataset.named_graphs.iter().any(|named| named == graph) =>
                        {
                            GraphPattern::Values(Vec::new())
                        }
                        _ => GraphPattern::Bgp(vec![triple]),
                    };
                    join_graph_patterns(current, item)
                })
        }
        GraphPattern::Join(left, right) => GraphPattern::Join(
            Box::new(apply_dataset_clauses(*left, dataset)),
            Box::new(apply_dataset_clauses(*right, dataset)),
        ),
        GraphPattern::LeftJoin(left, right) => GraphPattern::LeftJoin(
            Box::new(apply_dataset_clauses(*left, dataset)),
            Box::new(apply_dataset_clauses(*right, dataset)),
        ),
        GraphPattern::Minus(left, right) => GraphPattern::Minus(
            Box::new(apply_dataset_clauses(*left, dataset)),
            Box::new(apply_dataset_clauses(*right, dataset)),
        ),
        GraphPattern::Union(left, right) => GraphPattern::Union(
            Box::new(apply_dataset_clauses(*left, dataset)),
            Box::new(apply_dataset_clauses(*right, dataset)),
        ),
        GraphPattern::ZeroOrMorePath(path) => {
            apply_dataset_path(GraphPattern::ZeroOrMorePath(path), dataset)
        }
        GraphPattern::ZeroOrMoreSequencePath {
            pattern,
            predicates,
        } => apply_dataset_path(
            GraphPattern::ZeroOrMoreSequencePath {
                pattern,
                predicates,
            },
            dataset,
        ),
        GraphPattern::ZeroLengthPath(path) => {
            apply_dataset_path(GraphPattern::ZeroLengthPath(path), dataset)
        }
        GraphPattern::Subquery { .. } | GraphPattern::Values(_) | GraphPattern::Empty => pattern,
        GraphPattern::Bind(pattern, binds) => {
            GraphPattern::Bind(Box::new(apply_dataset_clauses(*pattern, dataset)), binds)
        }
        GraphPattern::DatasetGraphBind {
            pattern,
            variable,
            graph,
        } => GraphPattern::DatasetGraphBind {
            pattern: Box::new(apply_dataset_clauses(*pattern, dataset)),
            variable,
            graph,
        },
        GraphPattern::Scoped { pattern, hidden } => GraphPattern::Scoped {
            pattern: Box::new(apply_dataset_clauses(*pattern, dataset)),
            hidden,
        },
        GraphPattern::Filter(pattern, filters) => {
            GraphPattern::Filter(Box::new(apply_dataset_clauses(*pattern, dataset)), filters)
        }
        GraphPattern::Exists {
            pattern,
            exists,
            negated,
        } => GraphPattern::Exists {
            pattern: Box::new(apply_dataset_clauses(*pattern, dataset)),
            exists: Box::new(apply_dataset_clauses(*exists, dataset)),
            negated,
        },
        GraphPattern::FilterOrExists {
            pattern,
            filter,
            exists,
            negated,
        } => GraphPattern::FilterOrExists {
            pattern: Box::new(apply_dataset_clauses(*pattern, dataset)),
            filter,
            exists: Box::new(apply_dataset_clauses(*exists, dataset)),
            negated,
        },
    }
}

fn apply_dataset_path(pattern: GraphPattern, dataset: &DatasetClauses) -> GraphPattern {
    let graph = match &pattern {
        GraphPattern::ZeroOrMorePath(path) | GraphPattern::ZeroLengthPath(path) => {
            path.graph.clone()
        }
        GraphPattern::ZeroOrMoreSequencePath { pattern, .. } => pattern.graph.clone(),
        _ => unreachable!("只对 property path 调用 dataset 变换"),
    };
    match graph.as_deref() {
        None if dataset.declared && dataset.default_graphs.is_empty() => {
            GraphPattern::Values(Vec::new())
        }
        None if dataset.default_graphs.is_empty() => pattern,
        None => set_path_graph(
            pattern,
            Some(format!(
                "{DATASET_DEFAULT_PATH_GRAPHS}{}",
                dataset.default_graphs.join("\u{1f}")
            )),
        ),
        Some(variable) if dataset.declared && variable.starts_with('?') => dataset
            .named_graphs
            .iter()
            .cloned()
            .map(|named| GraphPattern::DatasetGraphBind {
                pattern: Box::new(set_path_graph(pattern.clone(), Some(named.clone()))),
                variable: variable.trim_start_matches('?').into(),
                graph: named,
            })
            .reduce(|left, right| GraphPattern::Union(Box::new(left), Box::new(right)))
            .unwrap_or_else(|| GraphPattern::Values(Vec::new())),
        Some(graph)
            if dataset.declared && !dataset.named_graphs.iter().any(|named| named == graph) =>
        {
            GraphPattern::Values(Vec::new())
        }
        _ => pattern,
    }
}

fn set_path_graph(mut pattern: GraphPattern, graph: Option<String>) -> GraphPattern {
    match &mut pattern {
        GraphPattern::ZeroOrMorePath(path) | GraphPattern::ZeroLengthPath(path) => {
            path.graph = graph
        }
        GraphPattern::ZeroOrMoreSequencePath { pattern, .. } => pattern.graph = graph,
        _ => unreachable!("只对 property path 设置 graph"),
    }
    pattern
}

fn validate_typed_boolean_literals(input: &str) -> Result<(), RuntimeError> {
    let pattern = Regex::new(r#"\"([^\"]*)\"\^\^<http://www.w3.org/2001/XMLSchema#boolean>"#)
        .expect("固定 boolean literal regex 有效");
    for captures in pattern.captures_iter(input) {
        let value = captures.get(1).expect("boolean literal 有 value").as_str();
        if !matches!(value, "true" | "false" | "1" | "0") {
            return Err(RuntimeError::Type(format!(
                "非法 xsd:boolean lexical form：{value}"
            )));
        }
    }
    Ok(())
}

fn validate_typed_datetime_literals(input: &str) -> Result<(), RuntimeError> {
    let pattern = Regex::new(r#"\"([^\"]*)\"\^\^<http://www.w3.org/2001/XMLSchema#dateTime>"#)
        .expect("固定 dateTime literal regex 有效");
    for captures in pattern.captures_iter(input) {
        let value = captures.get(1).expect("dateTime literal 有 value").as_str();
        if DateTime::parse_from_rfc3339(value).is_err()
            && NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f").is_err()
        {
            return Err(RuntimeError::Type(format!(
                "非法 xsd:dateTime lexical form：{value}"
            )));
        }
    }
    Ok(())
}

/// 将 SPARQL long string 的两种 delimiter 归一成后续 lexer 已支持的双引号。
/// value 本身（包括换行）不变；固定基线资产不含 delimiter 内的同型三连引号。
fn normalize_long_string_delimiters(input: &str) -> String {
    let characters = input.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(input.len());
    let mut index = 0usize;
    while index < characters.len() {
        let delimiter = matches!(characters.get(index..index + 3), Some(['"', '"', '"']))
            .then_some('"')
            .or_else(|| {
                matches!(characters.get(index..index + 3), Some(['\'', '\'', '\''])).then_some('\'')
            });
        let Some(delimiter) = delimiter else {
            output.push(characters[index]);
            index += 1;
            continue;
        };
        output.push('"');
        index += 3;
        let mut escaped = false;
        while index < characters.len() {
            if !escaped
                && characters.get(index..index + 3) == Some(&[delimiter, delimiter, delimiter])
            {
                output.push('"');
                index += 3;
                break;
            }
            let character = characters[index];
            if character == '"' && !escaped {
                output.push('\\');
            }
            if character == '\'' && delimiter == '\'' && escaped {
                output.pop();
                output.push('\'');
                escaped = false;
                index += 1;
                continue;
            }
            output.push(character);
            escaped = character == '\\' && !escaped;
            if character != '\\' {
                escaped = false;
            }
            index += 1;
        }
    }
    output
}

/// 解析器内部统一以双引号表示 RDF literal。将单引号形式转换时，保留原有
/// escape，并对 value 内未转义的双引号补 escape；`\\'` 则成为普通 apostrophe。
fn normalize_single_quoted_literals(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut escaped = false;
    for character in input.chars() {
        if single_quoted {
            if character == '\'' && !escaped {
                output.push('"');
                single_quoted = false;
                continue;
            }
            if character == '"' && !escaped {
                output.push('\\');
            }
            if character == '\'' && escaped {
                output.pop();
                output.push('\'');
                escaped = false;
                continue;
            }
            output.push(character);
            escaped = character == '\\' && !escaped;
            if character != '\\' {
                escaped = false;
            }
        } else if character == '"' && !escaped {
            output.push(character);
            double_quoted = !double_quoted;
            escaped = false;
        } else if character == '\'' && !double_quoted {
            output.push('"');
            single_quoted = true;
            escaped = false;
        } else {
            output.push(character);
            escaped = character == '\\' && !escaped;
            if character != '\\' {
                escaped = false;
            }
        }
    }
    output
}

fn normalize_dollar_variables(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut index = 0usize;
    let mut quoted = false;
    let mut iri = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'"' if !iri && !escaped => quoted = !quoted,
            b'<' if !quoted => iri = true,
            b'>' if iri => iri = false,
            b'$' if !quoted
                && !iri
                && bytes
                    .get(index + 1)
                    .is_some_and(|next| next.is_ascii_alphabetic() || *next == b'_') =>
            {
                output.push('?');
                index += 1;
                escaped = false;
                continue;
            }
            _ => {}
        }
        if byte.is_ascii() {
            output.push(byte as char);
        } else {
            let character = input[index..]
                .chars()
                .next()
                .expect("index is at a UTF-8 boundary");
            output.push(character);
            index += character.len_utf8();
            escaped = false;
            continue;
        }
        escaped = byte == b'\\' && !escaped;
        if byte != b'\\' {
            escaped = false;
        }
        index += 1;
    }
    output
}

/// SPARQL 的 bare `true`/`false` 是 xsd:boolean literal。仅在词法层的字符串和 IRI
/// 之外处理，避免影响 IRI local name 或 literal 内容。
fn normalize_bare_boolean_literals(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut index = 0usize;
    let mut quoted = false;
    let mut iri = false;
    while index < bytes.len() {
        match bytes[index] {
            b'"' if !iri => quoted = !quoted,
            b'<' if !quoted => iri = true,
            b'>' if iri => iri = false,
            _ => {}
        }
        if !quoted && !iri {
            for boolean in ["true", "false"] {
                if input[index..]
                    .get(..boolean.len())
                    .is_some_and(|candidate| candidate.eq_ignore_ascii_case(boolean))
                {
                    let before = index.checked_sub(1).and_then(|i| bytes.get(i));
                    let after = bytes.get(index + boolean.len());
                    let boundary = |byte: Option<&u8>| {
                        byte.is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
                    };
                    if boundary(before) && boundary(after) {
                        output.push_str(&format!(
                            "\"{boolean}\"^^<http://www.w3.org/2001/XMLSchema#boolean>"
                        ));
                        index += boolean.len();
                        continue;
                    }
                }
            }
        }
        if bytes[index].is_ascii() {
            output.push(bytes[index] as char);
            index += 1;
        } else {
            let character = input[index..]
                .chars()
                .next()
                .expect("index is at a UTF-8 boundary");
            output.push(character);
            index += character.len_utf8();
        }
    }
    output
}

/// 递归解析一个 WHERE/group graph pattern。这里刻意在语法层保留 JOIN 的左右顺序：
/// BIND、FILTER、OPTIONAL 和子查询都会在它们出现的位置变成代数节点，而不是在解析后
/// 从字符串中批量抽取。
fn parse_graph_pattern(input: &str) -> Result<GraphPattern, RuntimeError> {
    let open = input
        .find('{')
        .ok_or_else(|| RuntimeError::MalformedSparql("缺少 `{`".into()))?;
    let close = matching_brace(input, open)?;
    if !input[close + 1..].trim().is_empty() {
        return Err(RuntimeError::MalformedSparql(
            "图模式 `}` 后存在意外内容".into(),
        ));
    }
    let pattern = parse_group_content(&input[open + 1..close])?;
    validate_blank_node_label_scopes(&pattern)?;
    Ok(pattern)
}

/// SPARQL blank-node label 的作用域严格限于一个 Basic Graph Pattern；它不是可在
/// 嵌套 group、OPTIONAL、UNION、GRAPH 或 EXISTS 两侧复用的查询变量。
///
/// 解析器会把这些结构保留为不同的 `GraphPattern::Bgp` 节点，因此在 AST 完成后
/// 统一检查边界，既允许同一 BGP 内的重复 label，也不会依赖任何具体 DAWG 文件名。
fn validate_blank_node_label_scopes(pattern: &GraphPattern) -> Result<(), RuntimeError> {
    fn is_bgp_join_chain(pattern: &GraphPattern) -> bool {
        matches!(pattern, GraphPattern::Bgp(_))
            || matches!(pattern, GraphPattern::Join(left, right) if is_bgp_join_chain(left) && is_bgp_join_chain(right))
    }

    fn scopes(pattern: &GraphPattern) -> Vec<std::collections::BTreeSet<String>> {
        match pattern {
            GraphPattern::Bgp(triples) => {
                let mut local = std::collections::BTreeSet::new();
                for triple in triples {
                    for term in [&triple.subject, &triple.object] {
                        if term.starts_with("_:") {
                            local.insert(term.clone());
                        }
                    }
                }
                vec![local]
            }
            GraphPattern::Join(left, right) if matches!(left.as_ref(), GraphPattern::Filter(inner, _) if matches!(inner.as_ref(), GraphPattern::Bgp(_))) =>
            {
                // FILTER 是 TriplesBlock 内的附属约束；紧随它的 triples 仍属于
                // 同一 BGP（DAWG syntax-sparql4/syn-11）。
                let mut left_scopes = scopes(left);
                let mut right_scopes = scopes(right);
                if let (Some(left), Some(right)) = (left_scopes.last_mut(), right_scopes.first()) {
                    left.extend(right.iter().cloned());
                    right_scopes.remove(0);
                }
                left_scopes.extend(right_scopes);
                left_scopes
            }
            GraphPattern::Join(_, _) if is_bgp_join_chain(pattern) => {
                let mut merged = std::collections::BTreeSet::new();
                if let GraphPattern::Join(left, right) = pattern {
                    for scope in scopes(left).into_iter().chain(scopes(right)) {
                        merged.extend(scope);
                    }
                }
                vec![merged]
            }
            GraphPattern::Join(left, right)
            | GraphPattern::LeftJoin(left, right)
            | GraphPattern::Minus(left, right)
            | GraphPattern::Union(left, right) => {
                let mut output = scopes(left);
                output.extend(scopes(right));
                output
            }
            GraphPattern::Filter(pattern, _) if is_bgp_join_chain(pattern) => {
                // FILTER 现在在 parser 中延后包裹整个 group；它仍不切断原始
                // TriplesBlock 的 blank-node label scope（syntax-sparql3
                // syn-blabel-cross-filter、syntax-sparql4/syn-11）。
                let mut merged = std::collections::BTreeSet::new();
                for scope in scopes(pattern) {
                    merged.extend(scope);
                }
                vec![merged]
            }
            GraphPattern::Bind(pattern, _)
            | GraphPattern::DatasetGraphBind { pattern, .. }
            | GraphPattern::Scoped { pattern, .. }
            | GraphPattern::Filter(pattern, _) => scopes(pattern),
            GraphPattern::Exists {
                pattern, exists, ..
            } => {
                let mut output = scopes(pattern);
                output.extend(scopes(exists));
                output
            }
            GraphPattern::FilterOrExists {
                pattern, exists, ..
            } => {
                let mut output = scopes(pattern);
                output.extend(scopes(exists));
                output
            }
            // Subquery 是独立 query level；其 blank-node label 不与外层共享。
            GraphPattern::Subquery { .. } => Vec::new(),
            GraphPattern::Empty
            | GraphPattern::ZeroOrMorePath(_)
            | GraphPattern::ZeroOrMoreSequencePath { .. }
            | GraphPattern::ZeroLengthPath(_)
            | GraphPattern::Values(_) => Vec::new(),
        }
    }

    let mut seen = std::collections::BTreeSet::new();
    for scope in scopes(pattern) {
        if let Some(label) = scope.iter().find(|label| seen.contains(*label)) {
            return Err(RuntimeError::MalformedSparql(format!(
                "blank-node label {label} 不能跨 Basic Graph Pattern 复用"
            )));
        }
        seen.extend(scope);
    }
    Ok(())
}

fn parse_group_content(content: &str) -> Result<GraphPattern, RuntimeError> {
    // SPARQL 子查询可在外层 group 内直接以 `SELECT ... { ... }` 出现，而不必
    // 再包一层花括号（DAWG syntax-subquery-01）。
    if content
        .trim_start()
        .to_ascii_uppercase()
        .starts_with("SELECT")
    {
        let Query::Select {
            variables,
            pattern,
            aggregates,
            projection_binds,
            group_by,
            having,
            distinct,
            order_by,
            offset,
            limit,
        } = parse(content.trim())?
        else {
            unreachable!("SELECT prefix 必须解析为 Select")
        };
        return Ok(GraphPattern::Subquery {
            pattern: Box::new(pattern),
            variables,
            aggregates,
            projection_binds,
            group_by,
            having,
            distinct,
            order_by,
            offset,
            limit,
        });
    }
    let mut current = GraphPattern::Empty;
    // FILTER 在其所在 group 内与 triple 文本位置无关；延后包裹整个 group，
    // 使置于 BGP 之前的 filter 在变量完成绑定后再求值（DAWG algebra
    // filter-placement-2/3）。
    let mut group_filters = Vec::new();
    let mut index = 0usize;
    let mut path_variable_index = 0usize;
    while index < content.len() {
        index = skip_group_separators(content, index);
        if index == content.len() {
            break;
        }
        let (item, next) = if starts_keyword_at(content, index, "GRAPH") {
            let open = content[index..]
                .find('{')
                .map(|offset| index + offset)
                .ok_or_else(|| RuntimeError::MalformedSparql("GRAPH 缺少 `{`".into()))?;
            let close = matching_brace(content, open)?;
            let graph = content[index + "GRAPH".len()..open].trim();
            let graph = if graph.starts_with('?') {
                graph.to_owned()
            } else if graph.starts_with('<') && graph.ends_with('>') {
                graph.trim_matches(['<', '>']).to_owned()
            } else {
                return Err(RuntimeError::UnsupportedSparql(
                    "GRAPH 仅支持 IRI 或变量具名图".into(),
                ));
            };
            // GRAPH 的内部不只允许裸三元组；原始 DAWG exists03 在此包含
            // correlated FILTER EXISTS。先用通用 group parser 建立代数，再把
            // 当前 graph scope 递归施加到其 BGP/EXISTS，而非把 FILTER 当成
            // 三元组 token。
            let mut item = parse_group_content(&content[open + 1..close])?;
            apply_graph_scope(&mut item, &graph);
            // GRAPH 是独立 Basic Graph Pattern 边界；即使当前尚无外层 binding，
            // 也须保留该边界以禁止 blank-node label 跨图复用。
            (
                GraphPattern::Scoped {
                    pattern: Box::new(item),
                    hidden: Vec::new(),
                },
                close + 1,
            )
        } else if content.as_bytes()[index] == b'{' {
            let close = matching_brace(content, index)?;
            let inner = content[index + 1..close].trim();
            let item = if inner.to_ascii_uppercase().starts_with("SELECT") {
                let Query::Select {
                    variables,
                    pattern,
                    aggregates,
                    projection_binds,
                    group_by,
                    having,
                    distinct,
                    order_by,
                    offset,
                    limit,
                } = parse(inner)?
                else {
                    unreachable!("SELECT prefix was checked")
                };
                GraphPattern::Subquery {
                    pattern: Box::new(pattern),
                    variables,
                    aggregates,
                    projection_binds,
                    group_by,
                    having,
                    distinct,
                    order_by,
                    offset,
                    limit,
                }
            } else {
                parse_group_content(inner)?
            };
            // 显式 nested group 是独立 algebra operand：先独立求值、再由外层
            // Join 合并，而不是将已绑定变量直接灌入。这样 FILTER 不会看见外层
            // binding（filter-nested-2），同名 BGP 变量仍会在 join 时按 RDF term
            // compatibility 关联（join-scope-1）。
            let hidden = graph_pattern_variables(&current);
            let item = GraphPattern::Scoped {
                pattern: Box::new(item),
                hidden,
            };
            (item, close + 1)
        } else if starts_keyword_at(content, index, "OPTIONAL") {
            let open = skip_group_separators(content, index + "OPTIONAL".len());
            if content.as_bytes().get(open) != Some(&b'{') {
                return Err(RuntimeError::MalformedSparql(
                    "OPTIONAL 后缺少图模式".into(),
                ));
            }
            let close = matching_brace(content, open)?;
            let right = parse_group_content(&content[open + 1..close])?;
            current = GraphPattern::LeftJoin(Box::new(current), Box::new(right));
            // OPTIONAL 是独立 group graph pattern，闭合 `}` 后允许一个可选
            // 分隔点。DAWG boolean-effective-value/query-bev-5 使用 `} . FILTER`。
            index = skip_optional_group_dot(content, close + 1);
            continue;
        } else if starts_keyword_at(content, index, "MINUS") {
            let open = skip_group_separators(content, index + "MINUS".len());
            if content.as_bytes().get(open) != Some(&b'{') {
                return Err(RuntimeError::MalformedSparql("MINUS 后缺少图模式".into()));
            }
            let close = matching_brace(content, open)?;
            let right_content = content[open + 1..close].trim();
            let right = if right_content.to_ascii_uppercase().starts_with("SELECT") {
                let Query::Select {
                    variables,
                    pattern,
                    aggregates,
                    projection_binds,
                    group_by,
                    having,
                    distinct,
                    order_by,
                    offset,
                    limit,
                } = parse(right_content)?
                else {
                    unreachable!("SELECT prefix was checked")
                };
                GraphPattern::Subquery {
                    pattern: Box::new(pattern),
                    variables,
                    aggregates,
                    projection_binds,
                    group_by,
                    having,
                    distinct,
                    order_by,
                    offset,
                    limit,
                }
            } else {
                parse_group_content(right_content)?
            };
            current = GraphPattern::Minus(Box::new(current), Box::new(right));
            index = close + 1;
            continue;
        } else if starts_keyword_at(content, index, "BIND") {
            let (bind, next) = parse_bind_at(content, index)?;
            if graph_pattern_variables(&current).contains(&bind.variable) {
                return Err(RuntimeError::MalformedSparql(format!(
                    "BIND 目标变量 ?{} 已在当前图模式作用域中绑定",
                    bind.variable
                )));
            }
            // BIND 的变量作用域覆盖其所在的整个 group；因此即便 FILTER 文本上
            // 位于 BIND 之前，也必须在 alias 被绑定后求值（DAWG bind08）。
            current = match current {
                GraphPattern::Filter(pattern, filters) => {
                    GraphPattern::Filter(Box::new(GraphPattern::Bind(pattern, vec![bind])), filters)
                }
                pattern => GraphPattern::Bind(Box::new(pattern), vec![bind]),
            };
            index = next;
            continue;
        } else if starts_keyword_at(content, index, "FILTER") {
            if let Some((filter, exists, negated, next)) =
                parse_filter_or_exists_at(content, index)?
            {
                current = GraphPattern::FilterOrExists {
                    pattern: Box::new(current),
                    filter,
                    exists: Box::new(exists),
                    negated,
                };
                index = skip_optional_group_dot(content, next);
                continue;
            }
            if let Some((exists, negated, next)) = parse_filter_exists_at(content, index)? {
                // 固定 Ontop endpoint 对含内层 FILTER 的 correlated NOT EXISTS 不可
                // 翻译，并把它作为查询执行错误暴露。保留该边界而不是悄悄扩展语义。
                if negated && graph_pattern_contains_filter(&exists) {
                    return Err(RuntimeError::NotFullyTranslatable(
                        "Some of the variables in the EXISTS subquery are unbound".into(),
                    ));
                }
                current = GraphPattern::Exists {
                    pattern: Box::new(current),
                    exists: Box::new(exists),
                    negated,
                };
                index = next;
                continue;
            }
            let (filter, next) = parse_filter_at(content, index)?;
            group_filters.push(filter);
            // FILTER 是 group graph pattern 的独立项；其后的 `.` 是该项与
            // 后续 triples 的可选分隔符，而不是 BGP 内可静默忽略的空 triple。
            index = skip_optional_group_dot(content, next);
            continue;
        } else if starts_keyword_at(content, index, "VALUES") {
            let (values, next) = parse_values_at(content, index)?;
            current = join_graph_patterns(current, GraphPattern::Values(values));
            index = next;
            continue;
        } else if starts_keyword_at(content, index, "SERVICE") {
            return Err(RuntimeError::NotFullyTranslatable(
                "SERVICE 在固定 Ontop PostgreSQL 基线中未支持；仅允许独立受控 federation 票据实现"
                    .into(),
            ));
        } else {
            let end = next_group_construct(content, index);
            if end == index {
                return Err(RuntimeError::UnsupportedSparql("无法解析图模式项".into()));
            }
            let raw = content[index..end].trim();
            // Triple block 后的尾随 `;` 可位于随后的 FILTER 前；它不再引入一个
            // predicate/object continuation（DAWG syntax-sparql3/syn-07）。
            let raw = if ["OPTIONAL", "MINUS", "FILTER", "BIND", "VALUES", "GRAPH"]
                .iter()
                .any(|keyword| starts_keyword_at(content, end, keyword))
                && raw
                    .strip_suffix(';')
                    .map(str::trim_end)
                    .is_some_and(|before| tokens(before).is_ok_and(|terms| terms.len() >= 3))
            {
                raw.strip_suffix(';').map(str::trim_end).unwrap_or(raw)
            } else {
                raw
            };
            let patterns = group(&format!("{{{raw}}}"))?;
            (
                graph_pattern_from_triples(patterns, &mut path_variable_index)?,
                end,
            )
        };

        // UNION 的分支可以是任意 group pattern（包括 nested SELECT、VALUES 或 BIND）。
        let mut union = item;
        let mut after = skip_group_separators(content, next);
        while starts_keyword_at(content, after, "UNION") {
            let right_open = skip_group_separators(content, after + "UNION".len());
            if content.as_bytes().get(right_open) != Some(&b'{') {
                return Err(RuntimeError::MalformedSparql("UNION 后缺少 `{`".into()));
            }
            let right_close = matching_brace(content, right_open)?;
            let right_content = content[right_open + 1..right_close].trim();
            let right = if right_content.to_ascii_uppercase().starts_with("SELECT") {
                let Query::Select {
                    variables,
                    pattern,
                    aggregates,
                    projection_binds,
                    group_by,
                    having,
                    distinct,
                    order_by,
                    offset,
                    limit,
                } = parse(right_content)?
                else {
                    unreachable!("SELECT prefix was checked")
                };
                GraphPattern::Subquery {
                    pattern: Box::new(pattern),
                    variables,
                    aggregates,
                    projection_binds,
                    group_by,
                    having,
                    distinct,
                    order_by,
                    offset,
                    limit,
                }
            } else {
                parse_group_content(right_content)?
            };
            union = GraphPattern::Union(Box::new(union), Box::new(right));
            after = skip_group_separators(content, right_close + 1);
        }
        current = join_graph_patterns(current, union);
        index = after;
    }
    Ok(if group_filters.is_empty() {
        current
    } else {
        GraphPattern::Filter(Box::new(current), group_filters)
    })
}

fn graph_pattern_contains_filter(pattern: &GraphPattern) -> bool {
    match pattern {
        GraphPattern::Filter(_, _) => true,
        GraphPattern::Join(left, right)
        | GraphPattern::LeftJoin(left, right)
        | GraphPattern::Minus(left, right)
        | GraphPattern::Union(left, right) => {
            graph_pattern_contains_filter(left) || graph_pattern_contains_filter(right)
        }
        GraphPattern::Bind(pattern, _)
        | GraphPattern::DatasetGraphBind { pattern, .. }
        | GraphPattern::Scoped { pattern, .. }
        | GraphPattern::Exists { pattern, .. } => graph_pattern_contains_filter(pattern),
        GraphPattern::FilterOrExists {
            pattern, exists, ..
        } => graph_pattern_contains_filter(pattern) || graph_pattern_contains_filter(exists),
        GraphPattern::Subquery { pattern, .. } => graph_pattern_contains_filter(pattern),
        GraphPattern::Empty
        | GraphPattern::Bgp(_)
        | GraphPattern::ZeroOrMorePath(_)
        | GraphPattern::ZeroOrMoreSequencePath { .. }
        | GraphPattern::ZeroLengthPath(_)
        | GraphPattern::Values(_) => false,
    }
}

/// 将有界 property path 降为现有图模式代数。此处刻意保留每个 sequence 中间节点：
/// SPARQL path 的有界 sequence 与普通 join 一样保留不同中间节点带来的 bag 重数。
/// Ontop 基线的 pp11 正是这一反例，不能把终点对预先去重。
fn graph_pattern_from_triples(
    patterns: Vec<TriplePattern>,
    path_variable_index: &mut usize,
) -> Result<GraphPattern, RuntimeError> {
    // 连续的普通 triple 是一个 BGP，而不是语法层面的二元 JOIN。保留这一代数
    // 边界既符合 SPARQL 的 basic graph pattern，也让 runtime 能在安全条件满足时
    // 把整个 PostgreSQL BGP 规划为一条 SQL JOIN；有 property path 时仍逐项降解。
    if patterns
        .iter()
        .all(|pattern| !contains_property_path_operator(&pattern.predicate))
    {
        return Ok(GraphPattern::Bgp(patterns));
    }
    patterns
        .into_iter()
        .try_fold(GraphPattern::Empty, |current, pattern| {
            let item = property_path_graph_pattern(pattern, path_variable_index)?;
            Ok(join_graph_patterns(current, item))
        })
}

fn property_path_graph_pattern(
    pattern: TriplePattern,
    path_variable_index: &mut usize,
) -> Result<GraphPattern, RuntimeError> {
    if !contains_property_path_operator(&pattern.predicate) {
        return Ok(GraphPattern::Bgp(vec![pattern]));
    }
    property_path_to_graph_pattern(
        &pattern.subject,
        &pattern.predicate,
        &pattern.object,
        pattern.graph.as_deref(),
        path_variable_index,
    )
}

fn contains_property_path_operator(path: &str) -> bool {
    path.starts_with('^')
        || path.contains(['/', '|', '(', ')', '{', '}', '+', '*'])
        || path.ends_with('?')
}

fn property_path_to_graph_pattern(
    subject: &str,
    path: &str,
    object: &str,
    graph: Option<&str>,
    path_variable_index: &mut usize,
) -> Result<GraphPattern, RuntimeError> {
    let path = strip_property_path_parentheses(path.trim())?;
    // `^(p1/p2)` 表示 endpoint 交换后的正向内部路径。递归交给已有的
    // sequence/alternative 代数展开，避免手工反转路径字符串而破坏嵌套括号。
    if path.starts_with("^(") && matching_property_path_parenthesis(path, 1)? + 1 == path.len() {
        return property_path_to_graph_pattern(
            object,
            &path[1..],
            subject,
            graph,
            path_variable_index,
        );
    }
    if let Some((left, right)) = split_property_path(path, '|') {
        return Ok(GraphPattern::Union(
            Box::new(property_path_to_graph_pattern(
                subject,
                left,
                object,
                graph,
                path_variable_index,
            )?),
            Box::new(property_path_to_graph_pattern(
                subject,
                right,
                object,
                graph,
                path_variable_index,
            )?),
        ));
    }
    if let Some((left, right)) = split_property_path(path, '/') {
        let middle = format!("?__rtop_path_{}", *path_variable_index);
        *path_variable_index += 1;
        return Ok(join_graph_patterns(
            property_path_to_graph_pattern(subject, left, &middle, graph, path_variable_index)?,
            property_path_to_graph_pattern(&middle, right, object, graph, path_variable_index)?,
        ));
    }
    if let Some((base, repetitions)) = property_path_exact_repetitions(path) {
        if repetitions == 0 {
            return Ok(GraphPattern::ZeroLengthPath(TriplePattern {
                subject: subject.into(),
                predicate: base.into(),
                object: object.into(),
                graph: graph.map(str::to_owned),
            }));
        }
        let mut current = GraphPattern::Empty;
        let mut start = subject.to_owned();
        for index in 0..repetitions {
            let end = if index + 1 == repetitions {
                object.to_owned()
            } else {
                let value = format!("?__rtop_path_{}", *path_variable_index);
                *path_variable_index += 1;
                value
            };
            current = join_graph_patterns(
                current,
                property_path_to_graph_pattern(&start, base, &end, graph, path_variable_index)?,
            );
            start = end;
        }
        return Ok(current);
    }
    if let Some(excluded) = negated_property_set(path) {
        if excluded.is_empty() || excluded.iter().any(|predicate| !is_iri_token(predicate)) {
            return Err(RuntimeError::UnsupportedSparql(
                "negated property set 仅支持一个或多个 IRI predicate".into(),
            ));
        }
        let predicate_variable = format!("__rtop_path_predicate_{}", *path_variable_index);
        *path_variable_index += 1;
        let filters = excluded
            .into_iter()
            .map(|predicate| {
                Filter::Expression(Expression::Comparison {
                    operator: ComparisonOperator::NotEqual,
                    left: Box::new(Expression::Variable(predicate_variable.clone())),
                    right: Box::new(Expression::Iri(predicate.trim_matches(['<', '>']).into())),
                })
            })
            .collect();
        return Ok(GraphPattern::Filter(
            Box::new(GraphPattern::Bgp(vec![TriplePattern {
                subject: subject.into(),
                predicate: format!("?{predicate_variable}"),
                object: object.into(),
                graph: graph.map(str::to_owned),
            }])),
            filters,
        ));
    }
    if let Some(predicate) = path.strip_suffix('*') {
        // `(:p)*` 与 `:p*` 等价；外层括号属于量词的 base path，而不是
        // 复合路径的一部分。只在剥去 `*` 后再解包，避免把 `(p1/p2)*`
        // 误当成单谓词闭包（DAWG pp36）。
        let predicate = strip_property_path_parentheses(predicate)?;
        // `((:p)*)*` 与 `:p*` 等价。递归仍会在复合 base（例如
        // `(p1/p2)*`）处落入下面的单 IRI 限制，因而不会扩大已验收范围。
        if predicate.ends_with('*') {
            return property_path_to_graph_pattern(
                subject,
                predicate,
                object,
                graph,
                path_variable_index,
            );
        }
        if is_iri_token(predicate) {
            return Ok(GraphPattern::ZeroOrMorePath(TriplePattern {
                subject: subject.into(),
                predicate: predicate.into(),
                object: object.into(),
                graph: graph.map(str::to_owned),
            }));
        }
        if let Some(predicates) = iri_sequence(predicate) {
            return Ok(GraphPattern::ZeroOrMoreSequencePath {
                pattern: TriplePattern {
                    subject: subject.into(),
                    predicate: predicate.into(),
                    object: object.into(),
                    graph: graph.map(str::to_owned),
                },
                predicates,
            });
        }
        return Err(RuntimeError::UnsupportedSparql(
            "零或多次 property path 仅支持单一 IRI predicate".into(),
        ));
    }
    if path.ends_with(['+', '?']) {
        return Err(RuntimeError::UnsupportedSparql(
            "任意长度 property path 尚未由固定 Ontop PostgreSQL 基线验收".into(),
        ));
    }
    if let Some(predicate) = path.strip_prefix('^') {
        if predicate.is_empty() || !is_iri_token(predicate) {
            return Err(RuntimeError::UnsupportedSparql(
                "property path inverse 需要一个 IRI predicate".into(),
            ));
        }
        return Ok(GraphPattern::Bgp(vec![TriplePattern {
            subject: object.into(),
            predicate: predicate.into(),
            object: subject.into(),
            graph: graph.map(str::to_owned),
        }]));
    }
    if !is_iri_token(path) {
        return Err(RuntimeError::UnsupportedSparql(
            "property path 仅支持 IRI、inverse、alternative、sequence 与 {n}".into(),
        ));
    }
    Ok(GraphPattern::Bgp(vec![TriplePattern {
        subject: subject.into(),
        predicate: path.into(),
        object: object.into(),
        graph: graph.map(str::to_owned),
    }]))
}

/// 第一阶段只把纯 IRI sequence 提升为可闭包的一次关系。alternative、inverse、
/// optional 和其他量词仍保留既有的拒绝边界，避免本票悄悄扩大 property-path 范围。
fn iri_sequence(path: &str) -> Option<Vec<String>> {
    let mut remaining = path;
    let mut predicates = Vec::new();
    while let Some((left, right)) = split_property_path(remaining, '/') {
        if !is_iri_token(left.trim()) {
            return None;
        }
        predicates.push(left.trim().into());
        remaining = right;
    }
    (predicates.len() > 0 && is_iri_token(remaining.trim())).then(|| {
        predicates.push(remaining.trim().into());
        predicates
    })
}

fn is_iri_token(value: &str) -> bool {
    value
        .strip_prefix('<')
        .and_then(|inner| inner.strip_suffix('>'))
        .is_some_and(|inner| !inner.contains(['<', '>']))
}

fn strip_property_path_parentheses(mut path: &str) -> Result<&str, RuntimeError> {
    while path.starts_with('(') {
        let close = matching_property_path_parenthesis(path, 0)?;
        if close + 1 != path.len() {
            break;
        }
        path = path[1..close].trim();
    }
    Ok(path)
}

fn split_property_path(path: &str, operator: char) -> Option<(&str, &str)> {
    let mut depth = 0usize;
    let mut iri = false;
    for (index, character) in path.char_indices() {
        match character {
            '<' => iri = true,
            '>' => iri = false,
            '(' if !iri => depth += 1,
            ')' if !iri => depth = depth.saturating_sub(1),
            value if !iri && depth == 0 && value == operator => {
                return Some((&path[..index], &path[index + value.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
}

fn matching_property_path_parenthesis(input: &str, open: usize) -> Result<usize, RuntimeError> {
    let mut depth = 0usize;
    let mut iri = false;
    for (offset, character) in input[open..].char_indices() {
        match character {
            '<' => iri = true,
            '>' => iri = false,
            '(' if !iri => depth += 1,
            ')' if !iri => {
                depth -= 1;
                if depth == 0 {
                    return Ok(open + offset);
                }
            }
            _ => {}
        }
    }
    Err(RuntimeError::MalformedSparql(
        "property path 缺少 `)`".into(),
    ))
}

fn property_path_exact_repetitions(path: &str) -> Option<(&str, usize)> {
    let open = path.rfind('{')?;
    let repetitions = path[open + 1..].strip_suffix('}')?.parse().ok()?;
    Some((&path[..open], repetitions))
}

/// 将 `!(<p1>|<p2>)` 拆为同一层的 IRI exclusion set。这里刻意不把 inverse
/// property 或嵌套路径伪装成简单一跳 exclusion；它们需要独立的路径代数语义。
fn negated_property_set(path: &str) -> Option<Vec<&str>> {
    let inner = path
        .strip_prefix("!(")
        .and_then(|value| value.strip_suffix(')'))
        .or_else(|| path.strip_prefix('!'))?;
    let mut predicates = Vec::new();
    let mut remaining = inner;
    while let Some((left, right)) = split_property_path(remaining, '|') {
        predicates.push(left.trim());
        remaining = right;
    }
    predicates.push(remaining.trim());
    Some(predicates)
}

fn join_graph_patterns(left: GraphPattern, right: GraphPattern) -> GraphPattern {
    match left {
        GraphPattern::Empty => right,
        left => GraphPattern::Join(Box::new(left), Box::new(right)),
    }
}

fn skip_group_separators(input: &str, mut index: usize) -> usize {
    while input
        .as_bytes()
        .get(index)
        .is_some_and(u8::is_ascii_whitespace)
    {
        index += 1;
    }
    index
}

fn skip_optional_group_dot(input: &str, index: usize) -> usize {
    let index = skip_group_separators(input, index);
    if input.as_bytes().get(index) == Some(&b'.') {
        skip_group_separators(input, index + 1)
    } else {
        index
    }
}

fn starts_keyword_at(input: &str, index: usize, keyword: &str) -> bool {
    input[index..]
        .get(..keyword.len())
        .is_some_and(|value| value.eq_ignore_ascii_case(keyword))
        && input
            .as_bytes()
            .get(index.wrapping_sub(1))
            .is_none_or(|byte| byte.is_ascii_whitespace() || matches!(*byte, b'{' | b'}' | b'.'))
        && input
            .as_bytes()
            .get(index + keyword.len())
            .is_none_or(|byte| byte.is_ascii_whitespace() || matches!(*byte, b'{' | b'}' | b'('))
}

fn next_group_construct(input: &str, start: usize) -> usize {
    let mut quoted = false;
    let mut iri = false;
    for (offset, character) in input[start..].char_indices() {
        let index = start + offset;
        match character {
            '"' if !iri => quoted = !quoted,
            '<' if !quoted => iri = true,
            '>' if iri => iri = false,
            '{' if !quoted && !iri && !property_path_quantifier_at(input, index) => return index,
            _ if !quoted
                && !iri
                && [
                    "OPTIONAL", "MINUS", "BIND", "FILTER", "VALUES", "UNION", "GRAPH",
                ]
                .iter()
                .any(|keyword| starts_keyword_at(input, index, keyword)) =>
            {
                return index
            }
            _ => {}
        }
    }
    input.len()
}

fn property_path_quantifier_at(input: &str, open: usize) -> bool {
    let Some(close) = input[open + 1..].find('}').map(|offset| open + offset + 1) else {
        return false;
    };
    !input[open + 1..close].is_empty()
        && input[open + 1..close]
            .bytes()
            .all(|byte| byte.is_ascii_digit())
}

fn parse_bind_at(input: &str, start: usize) -> Result<(Bind, usize), RuntimeError> {
    let open = input[start + "BIND".len()..]
        .find('(')
        .map(|offset| start + "BIND".len() + offset)
        .ok_or_else(|| RuntimeError::MalformedSparql("BIND 缺少 `(`".into()))?;
    let close = matching_paren(input, open)?;
    let content = input[open + 1..close].trim();
    let as_at = content
        .to_ascii_uppercase()
        .rfind(" AS ")
        .ok_or_else(|| RuntimeError::MalformedSparql("BIND 缺少 AS ?variable".into()))?;
    Ok((
        Bind {
            variable: variable(content[as_at + 4..].trim())?,
            expression: parse_expression(content[..as_at].trim())?,
        },
        close + 1,
    ))
}

fn parse_filter_at(input: &str, start: usize) -> Result<(Filter, usize), RuntimeError> {
    let open = input[start + "FILTER".len()..]
        .find('(')
        .map(|offset| start + "FILTER".len() + offset)
        .ok_or_else(|| RuntimeError::MalformedSparql("FILTER 缺少 `(`".into()))?;
    let close = matching_paren(input, open)?;
    let (_, mut filters) = extract_filters(&input[start..=close])?;
    Ok((
        filters
            .pop()
            .expect("a complete FILTER expression produces one filter"),
        close + 1,
    ))
}

fn parse_filter_or_exists_at(
    input: &str,
    start: usize,
) -> Result<Option<(Filter, GraphPattern, bool, usize)>, RuntimeError> {
    let open = input[start + "FILTER".len()..]
        .find('(')
        .map(|offset| start + "FILTER".len() + offset);
    let Some(open) = open else {
        return Ok(None);
    };
    let close = matching_paren(input, open)?;
    let expression = input[open + 1..close].trim();
    let Some((left, right)) = expression.split_once("||") else {
        return Ok(None);
    };
    let mut right = right.trim();
    let negated = if starts_keyword_at(right, 0, "NOT") {
        right = right["NOT".len()..].trim_start();
        true
    } else {
        false
    };
    if !starts_keyword_at(right, 0, "EXISTS") {
        return Ok(None);
    }
    right = right["EXISTS".len()..].trim_start();
    if !right.starts_with('{') {
        return Err(RuntimeError::MalformedSparql(
            "FILTER EXISTS 后缺少 `{`".into(),
        ));
    }
    let exists_close = matching_brace(right, 0)?;
    if !right[exists_close + 1..].trim().is_empty() {
        return Ok(None);
    }
    Ok(Some((
        Filter::Expression(parse_expression(left.trim())?),
        parse_group_content(&right[1..exists_close])?,
        negated,
        close + 1,
    )))
}

fn parse_filter_exists_at(
    input: &str,
    start: usize,
) -> Result<Option<(GraphPattern, bool, usize)>, RuntimeError> {
    let mut index = skip_group_separators(input, start + "FILTER".len());
    // EXISTS 既可以是 FILTER 的直接操作数，也可以出现在括号内。后者在
    // DAWG syntax-exists-01/03 和 syntax-not-exists-01/03 中使用；`true &&`
    // 这一项可等价地归约为右侧 EXISTS，因而不需要把布尔表达式泄漏到代数层。
    if input.as_bytes().get(index) == Some(&b'(') {
        let close = matching_paren(input, index)?;
        if let Some((exists, negated)) =
            parse_parenthesized_filter_exists(&input[index + 1..close])?
        {
            return Ok(Some((exists, negated, close + 1)));
        }
        return Ok(None);
    }
    let negated = if starts_keyword_at(input, index, "NOT") {
        index = skip_group_separators(input, index + "NOT".len());
        true
    } else {
        false
    };
    if !starts_keyword_at(input, index, "EXISTS") {
        return Ok(None);
    }
    index = skip_group_separators(input, index + "EXISTS".len());
    if input.as_bytes().get(index) != Some(&b'{') {
        return Err(RuntimeError::MalformedSparql(
            "FILTER EXISTS 后缺少 `{`".into(),
        ));
    }
    let close = matching_brace(input, index)?;
    Ok(Some((
        parse_group_content(&input[index + 1..close])?,
        negated,
        close + 1,
    )))
}

/// 解析括号包裹的、可直接下降为 `GraphPattern::Exists` 的 FILTER expression。
/// 这里刻意只接受语义完全等价的 `!`、`NOT` 和 `true &&` 形式；其他复合布尔
/// expression 仍应走通用 expression parser，避免把未实现的执行语义伪装成 EXISTS。
fn parse_parenthesized_filter_exists(
    input: &str,
) -> Result<Option<(GraphPattern, bool)>, RuntimeError> {
    let compact = input
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .collect::<String>();
    let compact_upper = compact.to_ascii_uppercase();
    if compact_upper.starts_with("TRUE&&NOTEXISTS{")
        || compact_upper
            .starts_with("\"TRUE\"^^<HTTP://WWW.W3.ORG/2001/XMLSCHEMA#BOOLEAN>&&NOTEXISTS{")
    {
        let open = input
            .find('{')
            .expect("已由 compact 前缀确认 EXISTS 图模式开括号");
        let close = matching_brace(input, open)?;
        if input[close + 1..].trim().is_empty() {
            return Ok(Some((parse_group_content(&input[open + 1..close])?, true)));
        }
        return Ok(None);
    }
    let mut index = skip_group_separators(input, 0);
    // `starts_keyword_at` 的 token 边界适用于 group construct；这里 `TRUE`
    // 后紧跟 `&&` 是 expression token 边界，故显式检查该组合。
    if input[index..]
        .get(.."TRUE".len())
        .is_some_and(|value| value.eq_ignore_ascii_case("TRUE"))
    {
        index = skip_group_separators(input, index + "TRUE".len());
        if !input[index..].starts_with("&&") {
            return Ok(None);
        }
        index = skip_group_separators(input, index + 2);
    }
    let negated = if input.as_bytes().get(index) == Some(&b'!') {
        index = skip_group_separators(input, index + 1);
        true
    } else if starts_keyword_at(input, index, "NOT") {
        index = skip_group_separators(input, index + "NOT".len());
        true
    } else {
        false
    };
    if !starts_keyword_at(input, index, "EXISTS") {
        return Ok(None);
    }
    index = skip_group_separators(input, index + "EXISTS".len());
    if input.as_bytes().get(index) != Some(&b'{') {
        return Err(RuntimeError::MalformedSparql(
            "FILTER EXISTS 后缺少 `{`".into(),
        ));
    }
    let close = matching_brace(input, index)?;
    if !input[close + 1..].trim().is_empty() {
        return Ok(None);
    }
    Ok(Some((
        parse_group_content(&input[index + 1..close])?,
        negated,
    )))
}

fn parse_values_at(input: &str, start: usize) -> Result<(Vec<Binding>, usize), RuntimeError> {
    let mut index = skip_group_separators(input, start + "VALUES".len());
    if input.as_bytes().get(index) == Some(&b'(') {
        let variables_close = matching_paren(input, index)?;
        let variables = tokens(&input[index + 1..variables_close])?
            .into_iter()
            .map(variable)
            .collect::<Result<Vec<_>, _>>()?;
        index = skip_group_separators(input, variables_close + 1);
        if input.as_bytes().get(index) != Some(&b'{') {
            return Err(RuntimeError::MalformedSparql("VALUES 缺少 `{`".into()));
        }
        let close = matching_brace(input, index)?;
        let mut values = Vec::new();
        let mut item = index + 1;
        while item < close {
            item = skip_group_separators(input, item);
            if item == close {
                break;
            }
            if input.as_bytes().get(item) != Some(&b'(') {
                return Err(RuntimeError::MalformedSparql(
                    "VALUES tuple 缺少 `(`".into(),
                ));
            }
            let tuple_close = matching_paren(input, item)?;
            if tuple_close > close {
                return Err(RuntimeError::MalformedSparql(
                    "VALUES tuple 超出值列表".into(),
                ));
            }
            let terms = tokens(&input[item + 1..tuple_close])?;
            if terms.len() != variables.len() {
                return Err(RuntimeError::MalformedSparql(
                    "VALUES tuple 的值数量不匹配".into(),
                ));
            }
            let mut binding = Binding::new();
            for (variable, term) in variables.iter().zip(terms) {
                // SPARQL 1.1 VALUES 的 UNDEF 不是 RDF lexical；它表示该 row 的变量
                // 未绑定，因而在 join 时不施加该变量的等值约束。
                if !term.eq_ignore_ascii_case("UNDEF") {
                    binding.insert(variable.clone(), value_term(term)?);
                }
            }
            values.push(binding);
            item = tuple_close + 1;
        }
        return Ok((values, close + 1));
    }
    // 历史 BINDINGS `{}` 以及其 VALUES 等价形式允许零列、零行 data block。
    if input.as_bytes().get(index) == Some(&b'{') {
        let close = matching_brace(input, index)?;
        return Ok((Vec::new(), close + 1));
    }
    // 旧 BINDINGS 允许省略 tuple 变量列表外的括号：`?x ?y { (1 2) }`。
    // 归一化关键字后仍在这里保留该语法，并生成与 VALUES (?x ?y) 相同的 binding。
    let open = input[index..]
        .find('{')
        .map(|offset| index + offset)
        .ok_or_else(|| RuntimeError::MalformedSparql("VALUES 缺少 `{`".into()))?;
    let legacy_variables = tokens(&input[index..open])?
        .into_iter()
        .map(variable)
        .collect::<Result<Vec<_>, _>>()?;
    if legacy_variables.len() > 1 {
        let close = matching_brace(input, open)?;
        let mut values = Vec::new();
        let mut item = open + 1;
        while item < close {
            item = skip_group_separators(input, item);
            if item == close {
                break;
            }
            if input.as_bytes().get(item) != Some(&b'(') {
                return Err(RuntimeError::MalformedSparql(
                    "VALUES tuple 缺少 `(`".into(),
                ));
            }
            let tuple_close = matching_paren(input, item)?;
            let terms = tokens(&input[item + 1..tuple_close])?;
            if terms.len() != legacy_variables.len() {
                return Err(RuntimeError::MalformedSparql(
                    "VALUES tuple 的值数量不匹配".into(),
                ));
            }
            let mut binding = Binding::new();
            for (variable, term) in legacy_variables.iter().zip(terms) {
                if !term.eq_ignore_ascii_case("UNDEF") {
                    binding.insert(variable.clone(), value_term(term)?);
                }
            }
            values.push(binding);
            item = tuple_close + 1;
        }
        return Ok((values, close + 1));
    }
    let variable_end = input[index..]
        .find(char::is_whitespace)
        .map(|offset| index + offset)
        .ok_or_else(|| RuntimeError::MalformedSparql("VALUES 缺少值列表".into()))?;
    let variable = variable(&input[index..variable_end])?;
    index = skip_group_separators(input, variable_end);
    if input.as_bytes().get(index) != Some(&b'{') {
        return Err(RuntimeError::MalformedSparql("VALUES 缺少 `{`".into()));
    }
    let close = matching_brace(input, index)?;
    let values = tokens(&input[index + 1..close])?
        .into_iter()
        .map(|term| {
            let mut binding = Binding::new();
            binding.insert(variable.clone(), value_term(term)?);
            Ok(binding)
        })
        .collect::<Result<_, RuntimeError>>()?;
    Ok((values, close + 1))
}

pub(crate) fn graph_pattern_variables(pattern: &GraphPattern) -> Vec<String> {
    let variables = match pattern {
        GraphPattern::Empty => Vec::new(),
        GraphPattern::Bgp(patterns) => patterns.iter().flat_map(pattern_variables).collect(),
        GraphPattern::ZeroOrMorePath(pattern)
        | GraphPattern::ZeroOrMoreSequencePath { pattern, .. }
        | GraphPattern::ZeroLengthPath(pattern) => pattern_variables(pattern),
        GraphPattern::Join(left, right)
        | GraphPattern::LeftJoin(left, right)
        | GraphPattern::Minus(left, right) => {
            let mut variables = graph_pattern_variables(left);
            variables.extend(graph_pattern_variables(right));
            variables
        }
        // UNION branch 内 BIND 引入的变量仅在 branch 内有 lexical scope，不能
        // 因两侧恰好同名就提升为外层 SELECT * 投影（DAWG bind07）。
        GraphPattern::Union(left, right) => {
            let mut variables = graph_pattern_variables(left);
            variables.retain(|variable| !graph_pattern_bind_variables(left).contains(variable));
            let mut right_variables = graph_pattern_variables(right);
            right_variables
                .retain(|variable| !graph_pattern_bind_variables(right).contains(variable));
            variables.extend(right_variables);
            variables
        }
        GraphPattern::Subquery { variables, .. } => variables.clone(),
        GraphPattern::Values(values) => values
            .iter()
            .flat_map(|value| value.keys().cloned())
            .collect(),
        GraphPattern::Bind(pattern, binds) => {
            let mut variables = graph_pattern_variables(pattern);
            variables.extend(binds.iter().map(|bind| bind.variable.clone()));
            variables
        }
        GraphPattern::DatasetGraphBind {
            pattern, variable, ..
        } => {
            let mut variables = graph_pattern_variables(pattern);
            variables.push(variable.clone());
            variables
        }
        GraphPattern::Scoped { pattern, .. } => graph_pattern_variables(pattern),
        GraphPattern::Filter(pattern, _) => graph_pattern_variables(pattern),
        GraphPattern::Exists { pattern, .. } | GraphPattern::FilterOrExists { pattern, .. } => {
            graph_pattern_variables(pattern)
        }
    };
    // Property-path 降解产生的中间节点和谓词变量属于查询代数实现，不能被
    // `SELECT *` 当成用户可观察投影。保留前缀仅由 parser 生成。
    variables
        .into_iter()
        .filter(|variable| !variable.starts_with("__rtop_path_"))
        .collect()
}

pub(crate) fn graph_pattern_bind_variables(pattern: &GraphPattern) -> Vec<String> {
    match pattern {
        GraphPattern::Bind(pattern, binds) => {
            let mut variables = graph_pattern_bind_variables(pattern);
            variables.extend(binds.iter().map(|bind| bind.variable.clone()));
            variables
        }
        GraphPattern::Join(left, right)
        | GraphPattern::LeftJoin(left, right)
        | GraphPattern::Minus(left, right)
        | GraphPattern::Union(left, right) => {
            let mut variables = graph_pattern_bind_variables(left);
            variables.extend(graph_pattern_bind_variables(right));
            variables
        }
        GraphPattern::Scoped { pattern, .. }
        | GraphPattern::DatasetGraphBind { pattern, .. }
        | GraphPattern::Filter(pattern, _)
        | GraphPattern::Exists { pattern, .. }
        | GraphPattern::FilterOrExists { pattern, .. } => graph_pattern_bind_variables(pattern),
        GraphPattern::Empty
        | GraphPattern::Bgp(_)
        | GraphPattern::ZeroOrMorePath(_)
        | GraphPattern::ZeroOrMoreSequencePath { .. }
        | GraphPattern::ZeroLengthPath(_)
        | GraphPattern::Subquery { .. }
        | GraphPattern::Values(_) => Vec::new(),
    }
}

/// ASK 沿用固定 Ontop PostgreSQL 基线的 predicate-variable 拒绝边界；AST 已可
/// 表达子查询、FILTER 等其他图模式，故检查必须递归而非只观察顶层 BGP。
fn graph_pattern_has_predicate_variable(pattern: &GraphPattern) -> bool {
    match pattern {
        GraphPattern::Empty | GraphPattern::Values(_) => false,
        GraphPattern::Bgp(patterns) => patterns
            .iter()
            .any(|triple| triple.predicate.starts_with('?')),
        GraphPattern::ZeroOrMorePath(_)
        | GraphPattern::ZeroOrMoreSequencePath { .. }
        | GraphPattern::ZeroLengthPath(_) => false,
        GraphPattern::DatasetGraphBind { pattern, .. } => {
            graph_pattern_has_predicate_variable(pattern)
        }
        GraphPattern::Join(left, right)
        | GraphPattern::LeftJoin(left, right)
        | GraphPattern::Minus(left, right)
        | GraphPattern::Union(left, right) => {
            graph_pattern_has_predicate_variable(left)
                || graph_pattern_has_predicate_variable(right)
        }
        GraphPattern::Subquery { pattern, .. }
        | GraphPattern::Bind(pattern, _)
        | GraphPattern::Scoped { pattern, .. }
        | GraphPattern::Filter(pattern, _) => graph_pattern_has_predicate_variable(pattern),
        GraphPattern::Exists {
            pattern, exists, ..
        } => {
            graph_pattern_has_predicate_variable(pattern)
                || graph_pattern_has_predicate_variable(exists)
        }
        GraphPattern::FilterOrExists {
            pattern, exists, ..
        } => {
            graph_pattern_has_predicate_variable(pattern)
                || graph_pattern_has_predicate_variable(exists)
        }
    }
}

fn keyword_position(input: &str, keyword: &str) -> Option<usize> {
    let upper = input.to_ascii_uppercase();
    let mut offset = 0;
    while let Some(found) = upper[offset..].find(keyword) {
        let start = offset + found;
        let end = start + keyword.len();
        let boundary = |byte: Option<u8>| {
            byte.is_none_or(|byte| {
                byte.is_ascii_whitespace() || matches!(byte, b'{' | b'}' | b'.' | b';')
            })
        };
        if boundary(input.as_bytes().get(start.wrapping_sub(1)).copied())
            && boundary(input.as_bytes().get(end).copied())
        {
            return Some(start);
        }
        offset = end;
    }
    None
}

fn matching_brace(input: &str, open: usize) -> Result<usize, RuntimeError> {
    let mut depth = 0usize;
    for (offset, character) in input[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(open + offset);
                }
            }
            _ => {}
        }
    }
    Err(RuntimeError::MalformedSparql("VALUES 缺少 `}`".into()))
}

fn value_term(input: &str) -> Result<RdfTerm, RuntimeError> {
    if input.starts_with('<') && input.ends_with('>') {
        return Ok(RdfTerm::Iri(input[1..input.len() - 1].into()));
    }
    if input.starts_with('"') {
        let (value, suffix) = quoted_literal(input)?;
        let language = suffix.strip_prefix('@').map(str::to_owned);
        let datatype = suffix
            .strip_prefix("^^<")
            .and_then(|value| value.strip_suffix('>'))
            .map(str::to_owned);
        return Ok(RdfTerm::Literal {
            value,
            datatype,
            language,
        });
    }
    if input.parse::<i64>().is_ok() {
        return Ok(RdfTerm::Literal {
            value: input.into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
            language: None,
        });
    }
    if input.parse::<f64>().is_ok() {
        return Ok(RdfTerm::Literal {
            value: input.into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()),
            language: None,
        });
    }
    Err(RuntimeError::UnsupportedSparql(
        "VALUES 值必须为 IRI、literal 或数值".into(),
    ))
}

fn parse_select_projection(
    input: &str,
) -> Result<(Option<Vec<String>>, Vec<Bind>, Vec<Aggregate>, bool), RuntimeError> {
    let mut input = input.trim();
    if let Some(rest) = input.strip_suffix("WHERE") {
        input = rest.trim_end();
    }
    let distinct = input.to_ascii_uppercase().starts_with("DISTINCT")
        && input.as_bytes().get(8).is_none_or(u8::is_ascii_whitespace);
    if distinct {
        input = input[8..].trim_start();
    } else if input.to_ascii_uppercase().starts_with("REDUCED")
        && input.as_bytes().get(7).is_none_or(u8::is_ascii_whitespace)
    {
        // REDUCED 可以消除重复、也可以保留任意重复。因此保持 solution
        // sequence 本身就是合法实现；这里仅在词法层识别 modifier，而不把它
        // 误解析为 projection variable 或强行应用 DISTINCT。
        input = input[7..].trim_start();
    }
    if input == "*" {
        return Ok((None, Vec::new(), Vec::new(), distinct));
    }
    let mut variables = Vec::new();
    let mut binds = Vec::new();
    let mut aggregates = Vec::new();
    let mut index = 0usize;
    while index < input.len() {
        while input
            .as_bytes()
            .get(index)
            .is_some_and(u8::is_ascii_whitespace)
        {
            index += 1;
        }
        if index == input.len() {
            break;
        }
        if input.as_bytes()[index] == b'(' {
            let close = matching_paren(input, index)?;
            let content = input[index + 1..close].trim();
            let as_at = content.to_ascii_uppercase().rfind(" AS ").ok_or_else(|| {
                RuntimeError::MalformedSparql("SELECT expression 缺少 AS ?variable".into())
            })?;
            let variable = variable(content[as_at + 4..].trim())?;
            if let Some(aggregate) = parse_coalesce_aggregate(content[..as_at].trim(), &variable)? {
                aggregates.push(aggregate);
            } else {
                binds.push(Bind {
                    variable: variable.clone(),
                    expression: parse_expression(content[..as_at].trim())?,
                });
            }
            variables.push(variable);
            index = close + 1;
        } else {
            let end = input[index..]
                .find(char::is_whitespace)
                .map(|offset| index + offset)
                .unwrap_or(input.len());
            variables.push(variable(&input[index..end])?);
            index = end;
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    if let Some(variable) = variables
        .iter()
        .find(|variable| !seen.insert(variable.as_str()))
    {
        return Err(RuntimeError::MalformedSparql(format!(
            "SELECT projection 重复变量 ?{variable}"
        )));
    }
    Ok((Some(variables), binds, aggregates, distinct))
}

fn parse_aggregate(input: &str, variable: &str) -> Result<Option<Aggregate>, RuntimeError> {
    let Some(open) = input.find('(') else {
        return Ok(None);
    };
    if !input.ends_with(')') {
        return Ok(None);
    }
    let kind = match input[..open].trim().to_ascii_uppercase().as_str() {
        "COUNT" => AggregateKind::Count,
        "SUM" => AggregateKind::Sum,
        "AVG" => AggregateKind::Avg,
        "MIN" => AggregateKind::Min,
        "MAX" => AggregateKind::Max,
        "SAMPLE" => AggregateKind::Sample,
        "GROUP_CONCAT" => AggregateKind::GroupConcat,
        _ => return Ok(None),
    };
    let mut body = input[open + 1..input.len() - 1].trim();
    let distinct = body.to_ascii_uppercase().starts_with("DISTINCT")
        && body.as_bytes().get(8).is_none_or(u8::is_ascii_whitespace);
    if distinct {
        body = body[8..].trim_start();
    }
    let (expression, separator) = if matches!(kind, AggregateKind::GroupConcat) {
        let mut parts = body.splitn(2, ';');
        let expression = parts.next().unwrap_or_default().trim();
        let separator = parts
            .next()
            .map(str::trim)
            .map(|option| {
                let (name, value) = option.split_once('=').ok_or_else(|| {
                    RuntimeError::MalformedSparql("GROUP_CONCAT separator 缺少 `=`".into())
                })?;
                if !name.trim().eq_ignore_ascii_case("SEPARATOR") {
                    return Err(RuntimeError::MalformedSparql(
                        "GROUP_CONCAT 仅支持 SEPARATOR 选项".into(),
                    ));
                }
                let value = value.trim();
                if value.len() >= 2
                    && matches!(value.as_bytes().first(), Some(b'\'' | b'"'))
                    && value.as_bytes().first() == value.as_bytes().last()
                {
                    Ok(value[1..value.len() - 1].to_owned())
                } else {
                    Err(RuntimeError::MalformedSparql(
                        "GROUP_CONCAT separator 必须是字符串 literal".into(),
                    ))
                }
            })
            .transpose()?;
        (expression, separator)
    } else {
        (body, None)
    };
    let count_all = matches!(kind, AggregateKind::Count) && expression == "*";
    Ok(Some(Aggregate {
        variable: variable.into(),
        kind,
        // COUNT(*) 没有可求值的 expression；evaluator 由 count_all 选择其语义。
        expression: if count_all {
            Expression::String(String::new())
        } else {
            parse_expression(expression)?
        },
        count_all,
        distinct,
        separator,
        fallback: None,
    }))
}

fn parse_coalesce_aggregate(
    input: &str,
    variable: &str,
) -> Result<Option<Aggregate>, RuntimeError> {
    let input = input.trim();
    let Some(open) = input.find('(') else {
        return parse_aggregate(input, variable);
    };
    if !input[..open].trim().eq_ignore_ascii_case("COALESCE") || !input.ends_with(')') {
        return parse_aggregate(input, variable);
    }
    let arguments = split_expression_arguments(&input[open + 1..input.len() - 1])?;
    if arguments.len() != 2 {
        return Ok(None);
    }
    let Some(mut aggregate) = parse_aggregate(arguments[0].trim(), variable)? else {
        return Ok(None);
    };
    aggregate.fallback = Some(parse_expression(arguments[1].trim())?);
    Ok(Some(aggregate))
}

fn matching_paren(input: &str, open: usize) -> Result<usize, RuntimeError> {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for (offset, character) in input[open..].char_indices() {
        match character {
            '"' if !escaped => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => {
                depth -= 1;
                if depth == 0 {
                    return Ok(open + offset);
                }
            }
            _ => {}
        }
        escaped = character == '\\' && !escaped;
        if character != '\\' {
            escaped = false;
        }
    }
    Err(RuntimeError::MalformedSparql("表达式缺少 `)`".into()))
}

fn parse_expression(input: &str) -> Result<Expression, RuntimeError> {
    let input = strip_expression_parentheses(input.trim())?;
    // `||` 的优先级低于一元 `!`。必须先分割逻辑表达式，才能把
    // `!BOUND(?end) || ?asOf < ?end` 解析为 OR(Not(BOUND(...)), ...)，而不是
    // 将整个右侧错误吞进 NOT 的操作数。
    if let Some((left, operator, right)) = split_logical_expression(input) {
        return Ok(Expression::Logical {
            operator,
            left: Box::new(parse_expression(left)?),
            right: Box::new(parse_expression(right)?),
        });
    }
    if let Some(inner) = input.strip_prefix('!').filter(|_| !input.starts_with("!=")) {
        return Ok(Expression::Not(Box::new(parse_expression(inner.trim())?)));
    }
    // 比较运算符的优先级低于一元 +/-，但必须先识别整个 expression 的顶层
    // 比较边界；否则 `-?o = -2` 会被误解释为 `0 - (?o = -2)`。
    if let Some((left, operator, right)) = split_comparison_expression(input) {
        return Ok(Expression::Comparison {
            operator,
            left: Box::new(parse_expression(left)?),
            right: Box::new(parse_expression(right)?),
        });
    }
    // 一元 +/- 仅改变数值 expression；复用既有二元算术 AST，避免让
    // `-?o`、`-(?a + ?b)` 走一套与普通减法不同的 evaluator。带符号数值由
    // 随后的 literal 分支直接解析，因此这里仅处理非字面量输入。
    if let Some(inner) = input
        .strip_prefix('+')
        .filter(|_| input.parse::<f64>().is_err())
    {
        return parse_expression(inner.trim());
    }
    if let Some(inner) = input
        .strip_prefix('-')
        .filter(|_| input.parse::<f64>().is_err())
    {
        return Ok(Expression::Binary {
            operator: ArithmeticOperator::Subtract,
            left: Box::new(Expression::Number("0".into())),
            right: Box::new(parse_expression(inner.trim())?),
        });
    }
    if let Some(variable) = input.strip_prefix('?').filter(|value| {
        !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    }) {
        return Ok(Expression::Variable(variable.into()));
    }
    if let Some((value, datatype)) = expression_typed_literal(input) {
        return Ok(Expression::TypedLiteral { value, datatype });
    }
    if let Some((value, language)) = expression_language_literal(input) {
        return Ok(Expression::LanguageLiteral { value, language });
    }
    if let Some(value) = expression_string_literal(input) {
        return Ok(Expression::String(value));
    }
    if input.starts_with('<') && input.ends_with('>') {
        return Ok(Expression::Iri(input[1..input.len() - 1].into()));
    }
    if matches!(input.to_ascii_lowercase().as_str(), "true" | "false") {
        return Ok(Expression::TypedLiteral {
            value: input.to_ascii_lowercase(),
            datatype: "http://www.w3.org/2001/XMLSchema#boolean".into(),
        });
    }
    if input.parse::<f64>().is_ok() {
        return Ok(Expression::Number(input.into()));
    }
    if let Some((left, negated, candidates)) = split_in_expression(input) {
        return Ok(Expression::In {
            value: Box::new(parse_expression(left)?),
            candidates: split_expression_arguments(candidates)?
                .into_iter()
                .map(parse_expression)
                .collect::<Result<_, _>>()?,
            negated,
        });
    }
    if let Some((left, operator, right)) = split_arithmetic_expression(input) {
        return Ok(Expression::Binary {
            operator,
            left: Box::new(parse_expression(left)?),
            right: Box::new(parse_expression(right)?),
        });
    }
    let upper = input.to_ascii_uppercase();
    if upper.starts_with("REPLACE(") && input.ends_with(')') {
        let arguments = split_expression_arguments(&input[8..input.len() - 1])?;
        if arguments.len() != 3 {
            return Err(RuntimeError::MalformedSparql("REPLACE 需要三个参数".into()));
        }
        return Ok(Expression::Replace {
            value: Box::new(parse_expression(arguments[0])?),
            pattern: Box::new(parse_expression(arguments[1])?),
            replacement: Box::new(parse_expression(arguments[2])?),
        });
    }
    if let Some(open) = input.find('(') {
        let name = input[..open].trim();
        if input.ends_with(')')
            && !name.is_empty()
            && (name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b':'))
                || (name.starts_with('<') && name.ends_with('>')))
        {
            let kind = match name.to_ascii_uppercase().as_str() {
                "COUNT" => Some(AggregateKind::Count),
                "SUM" => Some(AggregateKind::Sum),
                "AVG" => Some(AggregateKind::Avg),
                "MIN" => Some(AggregateKind::Min),
                "MAX" => Some(AggregateKind::Max),
                "SAMPLE" => Some(AggregateKind::Sample),
                _ => None,
            };
            let arguments = split_expression_arguments(&input[open + 1..input.len() - 1])?;
            if let Some(kind) = kind {
                if arguments.len() != 1 {
                    return Err(RuntimeError::MalformedSparql(
                        "嵌套 aggregate 需要一个 expression".into(),
                    ));
                }
                let argument = arguments[0].trim();
                let distinct = argument.to_ascii_uppercase().starts_with("DISTINCT")
                    && argument
                        .as_bytes()
                        .get(8)
                        .is_none_or(u8::is_ascii_whitespace);
                let argument = distinct
                    .then(|| argument[8..].trim_start())
                    .unwrap_or(argument);
                let count_all = matches!(kind, AggregateKind::Count) && argument == "*";
                return Ok(Expression::Aggregate {
                    kind,
                    expression: Box::new(if count_all {
                        Expression::String(String::new())
                    } else {
                        parse_expression(argument)?
                    }),
                    count_all,
                    distinct,
                });
            }
            return Ok(Expression::Function {
                name: name.to_ascii_uppercase(),
                arguments: arguments
                    .into_iter()
                    .map(parse_expression)
                    .collect::<Result<_, _>>()?,
                base_iri: None,
            });
        }
    }
    Err(RuntimeError::UnsupportedSparql(format!(
        "不支持的 BIND expression：{input}"
    )))
}

fn extract_base(input: &str) -> Result<(String, Option<String>), RuntimeError> {
    let remaining = input.trim_start();
    let upper = remaining.get(..4).map(str::to_ascii_uppercase);
    if upper.as_deref() != Some("BASE")
        || remaining
            .as_bytes()
            .get(4)
            .is_some_and(|byte| !byte.is_ascii_whitespace())
    {
        return Ok((input.into(), None));
    }
    let line_end = remaining.find('\n').unwrap_or(remaining.len());
    let declaration = remaining[..line_end].trim();
    let terms = declaration.split_whitespace().collect::<Vec<_>>();
    if terms.len() != 2 || !terms[1].starts_with('<') || !terms[1].ends_with('>') {
        return Err(RuntimeError::MalformedSparql("无效 BASE 声明".into()));
    }
    let base = terms[1][1..terms[1].len() - 1].to_owned();
    oxiri::Iri::parse(base.clone())
        .map_err(|_| RuntimeError::MalformedSparql("BASE 必须是绝对 IRI".into()))?;
    Ok((remaining[line_end..].trim_start().into(), Some(base)))
}

/// 将一个 GRAPH 块的活动 graph 递归施加给其内部代数。FILTER/EXISTS 自身不
/// 携带 graph，但它们求值的左右图模式必须继承该 lexical graph scope。
fn apply_graph_scope(pattern: &mut GraphPattern, graph: &str) {
    match pattern {
        GraphPattern::Empty | GraphPattern::Values(_) => {}
        GraphPattern::Bgp(patterns) => {
            for triple in patterns {
                triple.graph = Some(graph.into());
            }
        }
        GraphPattern::ZeroOrMorePath(pattern)
        | GraphPattern::ZeroOrMoreSequencePath { pattern, .. }
        | GraphPattern::ZeroLengthPath(pattern) => pattern.graph = Some(graph.into()),
        GraphPattern::Join(left, right)
        | GraphPattern::LeftJoin(left, right)
        | GraphPattern::Minus(left, right)
        | GraphPattern::Union(left, right) => {
            apply_graph_scope(left, graph);
            apply_graph_scope(right, graph);
        }
        // 子查询在 GRAPH 作用域中独立求值。若图变量没有被子查询投影，不能让
        // 它与子查询内恰好同名、但不向外可见的变量相等（DAWG sq03）；反之，
        // SELECT * 投影了该变量时，二者必须是同一个 binding（DAWG sq02）。
        // parser 已在施加外层 GRAPH scope 前固定了子查询 projection，因此用
        // 一个仅执行期可见的图变量即可表达前一情形。
        GraphPattern::Subquery {
            pattern, variables, ..
        } => {
            let scope = if graph.starts_with('?')
                && !variables.iter().any(|variable| variable == &graph[1..])
            {
                format!("?__rtop_graph_scope_{}", &graph[1..])
            } else {
                graph.into()
            };
            apply_graph_scope(pattern, &scope);
        }
        GraphPattern::Bind(pattern, _)
        | GraphPattern::DatasetGraphBind { pattern, .. }
        | GraphPattern::Scoped { pattern, .. }
        | GraphPattern::Filter(pattern, _) => apply_graph_scope(pattern, graph),
        GraphPattern::Exists {
            pattern, exists, ..
        } => {
            apply_graph_scope(pattern, graph);
            apply_graph_scope(exists, graph);
        }
        GraphPattern::FilterOrExists {
            pattern, exists, ..
        } => {
            apply_graph_scope(pattern, graph);
            apply_graph_scope(exists, graph);
        }
    }
}

fn set_pattern_base_iri(pattern: &mut GraphPattern, base_iri: Option<&str>) {
    match pattern {
        GraphPattern::Empty | GraphPattern::Values(_) => {}
        GraphPattern::Bgp(patterns) => {
            for triple in patterns {
                resolve_pattern_iris(triple, base_iri);
            }
        }
        GraphPattern::ZeroOrMorePath(pattern) | GraphPattern::ZeroLengthPath(pattern) => {
            resolve_pattern_iris(pattern, base_iri)
        }
        GraphPattern::ZeroOrMoreSequencePath {
            pattern,
            predicates,
        } => {
            resolve_pattern_iris(pattern, base_iri);
            for predicate in predicates {
                let mut predicate_pattern = TriplePattern {
                    subject: "?s".into(),
                    predicate: predicate.clone(),
                    object: "?o".into(),
                    graph: None,
                };
                resolve_pattern_iris(&mut predicate_pattern, base_iri);
                *predicate = predicate_pattern.predicate;
            }
        }
        GraphPattern::Join(left, right)
        | GraphPattern::LeftJoin(left, right)
        | GraphPattern::Minus(left, right)
        | GraphPattern::Union(left, right) => {
            set_pattern_base_iri(left, base_iri);
            set_pattern_base_iri(right, base_iri);
        }
        GraphPattern::Subquery { pattern, .. } => set_pattern_base_iri(pattern, base_iri),
        GraphPattern::Bind(pattern, binds) => {
            set_pattern_base_iri(pattern, base_iri);
            for bind in binds {
                set_expression_base_iri(&mut bind.expression, base_iri);
            }
        }
        GraphPattern::DatasetGraphBind { pattern, .. } => set_pattern_base_iri(pattern, base_iri),
        GraphPattern::Scoped { pattern, .. } => set_pattern_base_iri(pattern, base_iri),
        GraphPattern::Filter(pattern, filters) => {
            set_pattern_base_iri(pattern, base_iri);
            for filter in filters {
                if let Filter::Expression(expression) = filter {
                    set_expression_base_iri(expression, base_iri);
                }
            }
        }
        GraphPattern::Exists {
            pattern, exists, ..
        } => {
            set_pattern_base_iri(pattern, base_iri);
            set_pattern_base_iri(exists, base_iri);
        }
        GraphPattern::FilterOrExists {
            pattern,
            filter,
            exists,
            ..
        } => {
            set_pattern_base_iri(pattern, base_iri);
            if let Filter::Expression(expression) = filter {
                set_expression_base_iri(expression, base_iri);
            }
            set_pattern_base_iri(exists, base_iri);
        }
    }
}

/// `BASE` 同样作用于 BGP 与 GRAPH 的相对 IRI。GRAPH 的内部表示不保留尖括号，
/// 因而这里单独按 IRI reference 解析它。
fn resolve_pattern_iris(pattern: &mut TriplePattern, base_iri: Option<&str>) {
    let Some(base_iri) = base_iri else {
        return;
    };
    for token in [
        &mut pattern.subject,
        &mut pattern.predicate,
        &mut pattern.object,
    ] {
        if let Some(reference) = token
            .strip_prefix('<')
            .and_then(|value| value.strip_suffix('>'))
        {
            if oxiri::Iri::parse(reference.to_owned()).is_err() {
                if let Ok(resolved) = oxiri::IriRef::from(
                    oxiri::Iri::parse(base_iri.to_owned()).expect("BASE 已在解析时验证"),
                )
                .resolve(reference)
                {
                    *token = format!("<{}>", resolved.into_inner());
                }
            }
        }
    }
    if let Some(graph) = &mut pattern.graph {
        if !graph.starts_with('?')
            && !graph.starts_with(DATASET_DEFAULT_PATH_GRAPHS)
            && oxiri::Iri::parse(graph.clone()).is_err()
        {
            if let Ok(resolved) = oxiri::IriRef::from(
                oxiri::Iri::parse(base_iri.to_owned()).expect("BASE 已在解析时验证"),
            )
            .resolve(graph.as_str())
            {
                *graph = resolved.into_inner();
            }
        }
    }
}

fn set_expression_base_iri(expression: &mut Expression, base_iri: Option<&str>) {
    match expression {
        Expression::Not(inner) => set_expression_base_iri(inner, base_iri),
        Expression::Binary { left, right, .. }
        | Expression::Logical { left, right, .. }
        | Expression::Comparison { left, right, .. } => {
            set_expression_base_iri(left, base_iri);
            set_expression_base_iri(right, base_iri);
        }
        Expression::In {
            value, candidates, ..
        } => {
            set_expression_base_iri(value, base_iri);
            for candidate in candidates {
                set_expression_base_iri(candidate, base_iri);
            }
        }
        Expression::Function {
            name,
            arguments,
            base_iri: function_base,
        } => {
            if matches!(name.as_str(), "IRI" | "URI") {
                *function_base = base_iri.map(str::to_owned);
            }
            for argument in arguments {
                set_expression_base_iri(argument, base_iri);
            }
        }
        Expression::Replace {
            value,
            pattern,
            replacement,
        } => {
            set_expression_base_iri(value, base_iri);
            set_expression_base_iri(pattern, base_iri);
            set_expression_base_iri(replacement, base_iri);
        }
        Expression::Aggregate { expression, .. } => set_expression_base_iri(expression, base_iri),
        Expression::Iri(value) => {
            if let Some(base_iri) = base_iri {
                if oxiri::Iri::parse(value.clone()).is_err() {
                    if let Ok(resolved) = oxiri::IriRef::from(
                        oxiri::Iri::parse(base_iri.to_owned()).expect("BASE 已在解析时验证"),
                    )
                    .resolve(value.as_str())
                    {
                        *value = resolved.into_inner();
                    }
                }
            }
        }
        Expression::Variable(_)
        | Expression::String(_)
        | Expression::LanguageLiteral { .. }
        | Expression::TypedLiteral { .. }
        | Expression::Number(_) => {}
    }
}

/// 在最外层识别 SPARQL 1.1 `IN`/`NOT IN`。列表中的 expression 留给既有
/// 参数拆分器处理，因此嵌套函数、括号和带逗号的字符串不会改变列表边界。
fn split_in_expression(input: &str) -> Option<(&str, bool, &str)> {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut iri = false;
    for (index, character) in input.char_indices() {
        match character {
            '\'' | '"' => quoted = !quoted,
            '<' if !quoted && starts_iri(input, index) => iri = true,
            '>' if iri => iri = false,
            '(' if !quoted && !iri => depth += 1,
            ')' if !quoted && !iri => depth = depth.checked_sub(1)?,
            _ if quoted || iri || depth != 0 => {}
            _ => {
                let before = input[..index].chars().next_back();
                if before
                    .is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
                {
                    continue;
                }
                let tail = &input[index..];
                let upper = tail.to_ascii_uppercase();
                let list_boundary = |byte: Option<&u8>| {
                    byte.is_none_or(|byte| byte.is_ascii_whitespace() || *byte == b'(')
                };
                let (negated, width) =
                    if upper.starts_with("NOT IN") && list_boundary(upper.as_bytes().get(6)) {
                        (true, 6)
                    } else if upper.starts_with("IN") && list_boundary(upper.as_bytes().get(2)) {
                        (false, 2)
                    } else {
                        continue;
                    };
                let list = tail[width..].trim_start();
                if !list.starts_with('(') || matching_paren(list, 0).ok()? != list.len() - 1 {
                    continue;
                }
                return Some((input[..index].trim_end(), negated, &list[1..list.len() - 1]));
            }
        }
    }
    None
}

fn strip_expression_parentheses(input: &str) -> Result<&str, RuntimeError> {
    if input.starts_with('(') && matching_paren(input, 0)? == input.len() - 1 {
        return strip_expression_parentheses(input[1..input.len() - 1].trim());
    }
    Ok(input)
}

fn split_arithmetic_expression(input: &str) -> Option<(&str, ArithmeticOperator, &str)> {
    for operators in ["+-", "*/"] {
        let mut depth = 0usize;
        let mut quoted = false;
        let mut iri = false;
        let mut candidate = None;
        for (index, character) in input.char_indices() {
            match character {
                '\'' | '"' => quoted = !quoted,
                '<' if !quoted && starts_iri(input, index) => iri = true,
                '>' if iri => iri = false,
                '(' if !quoted && !iri => depth += 1,
                ')' if !quoted && !iri => depth = depth.checked_sub(1)?,
                _ if quoted || iri || depth != 0 => {}
                _ if operators.contains(character) => {
                    let preceding = input[..index].chars().next_back();
                    if !matches!(preceding, None | Some('(' | '+' | '-' | '*' | '/')) {
                        candidate = Some((index, character));
                    }
                }
                _ => {}
            }
        }
        if let Some((index, operator)) = candidate {
            let operator = match operator {
                '+' => ArithmeticOperator::Add,
                '-' => ArithmeticOperator::Subtract,
                '*' => ArithmeticOperator::Multiply,
                '/' => ArithmeticOperator::Divide,
                _ => unreachable!(),
            };
            return Some((&input[..index], operator, &input[index + 1..]));
        }
    }
    None
}

fn split_logical_expression(input: &str) -> Option<(&str, LogicalOperator, &str)> {
    // SPARQL 的 `&&` 比 `||` 优先；每层取最右侧运算符可保留左结合。
    for (needle, operator) in [("||", LogicalOperator::Or), ("&&", LogicalOperator::And)] {
        let mut depth = 0usize;
        let mut quoted = false;
        let mut iri = false;
        let mut candidate = None;
        for (index, character) in input.char_indices() {
            match character {
                '\'' | '"' => quoted = !quoted,
                '<' if !quoted && starts_iri(input, index) => iri = true,
                '>' if iri => iri = false,
                '(' if !quoted && !iri => depth += 1,
                ')' if !quoted && !iri => depth = depth.checked_sub(1)?,
                _ if !quoted && !iri && depth == 0 && input[index..].starts_with(needle) => {
                    candidate = Some(index);
                }
                _ => {}
            }
        }
        if let Some(index) = candidate {
            return Some((&input[..index], operator, &input[index + needle.len()..]));
        }
    }
    None
}

fn split_comparison_expression(input: &str) -> Option<(&str, ComparisonOperator, &str)> {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut iri = false;
    for (index, character) in input.char_indices() {
        match character {
            '\'' | '"' => quoted = !quoted,
            '<' if !quoted && starts_iri(input, index) => iri = true,
            '>' if iri => iri = false,
            '(' if !quoted && !iri => depth += 1,
            ')' if !quoted && !iri => depth = depth.checked_sub(1)?,
            '=' | '<' | '>' if !quoted && !iri && depth == 0 => {
                let (operator, width, left_end) = match character {
                    '=' if input[..index].ends_with('!') => {
                        (ComparisonOperator::NotEqual, 1, index - 1)
                    }
                    '=' if input[..index].ends_with('<') => {
                        (ComparisonOperator::LessOrEqual, 1, index - 1)
                    }
                    '=' if input[..index].ends_with('>') => {
                        (ComparisonOperator::GreaterOrEqual, 1, index - 1)
                    }
                    '=' => (ComparisonOperator::Equal, 1, index),
                    '<' if input[index + 1..].starts_with('=') => {
                        (ComparisonOperator::LessOrEqual, 2, index)
                    }
                    '>' if input[index + 1..].starts_with('=') => {
                        (ComparisonOperator::GreaterOrEqual, 2, index)
                    }
                    '<' => (ComparisonOperator::Less, 1, index),
                    '>' => (ComparisonOperator::Greater, 1, index),
                    _ => unreachable!(),
                };
                if matches!(character, '=')
                    && (input[..index].ends_with('<') || input[..index].ends_with('>'))
                {
                    continue;
                }
                return Some((&input[..left_end], operator, &input[index + width..]));
            }
            _ => {}
        }
    }
    None
}

fn expression_string_literal(input: &str) -> Option<String> {
    let quote = input.chars().next()?;
    if !matches!(quote, '\'' | '"') || input.len() < 2 {
        return None;
    }
    let end = input[1..].find(quote)? + 1;
    let suffix = &input[end + 1..];
    if !suffix.is_empty()
        && !(suffix.starts_with("^^<")
            && suffix[3..]
                .find('>')
                .is_some_and(|datatype_end| datatype_end + 4 == suffix.len()))
    {
        return None;
    }
    Some(
        input[1..end]
            .replace(&format!("\\{quote}"), &quote.to_string())
            // SPARQL `\\` 在 string literal 中表示一个反斜杠；REGEX 的
            // `"example\\\\.com"` 因而必须把 `\.` 交给 regex engine。
            .replace("\\\\", "\\"),
    )
}

fn expression_language_literal(input: &str) -> Option<(String, String)> {
    let quote = input.chars().next()?;
    if !matches!(quote, '\'' | '"') || input.len() < 2 {
        return None;
    }
    let end = input[1..].find(quote)? + 1;
    let language = input[end + 1..].strip_prefix('@')?;
    (!language.is_empty()
        && language
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'))
    .then(|| {
        (
            input[1..end].replace(&format!("\\{quote}"), &quote.to_string()),
            language.to_owned(),
        )
    })
}

fn expression_typed_literal(input: &str) -> Option<(String, String)> {
    let quote = input.chars().next()?;
    if !matches!(quote, '\'' | '"') || input.len() < 2 {
        return None;
    }
    let end = input[1..].find(quote)? + 1;
    let suffix = input[end + 1..].strip_prefix("^^<")?;
    let datatype_end = suffix.find('>')?;
    if datatype_end + 1 != suffix.len() {
        return None;
    }
    let datatype = &suffix[..datatype_end];
    (!datatype.is_empty()).then(|| {
        (
            input[1..end].replace(&format!("\\{quote}"), &quote.to_string()),
            datatype.into(),
        )
    })
}

fn split_expression_arguments(input: &str) -> Result<Vec<&str>, RuntimeError> {
    if input.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut arguments = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut quoted = false;
    let mut iri = false;
    for (index, character) in input.char_indices() {
        match character {
            '"' => quoted = !quoted,
            '<' if !quoted && starts_iri(input, index) => iri = true,
            '>' if iri => iri = false,
            '(' if !quoted && !iri => depth += 1,
            ')' if !quoted && !iri => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| RuntimeError::MalformedSparql("表达式多余 `)`".into()))?
            }
            ',' if !quoted && !iri && depth == 0 => {
                arguments.push(input[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    arguments.push(input[start..].trim());
    Ok(arguments)
}

fn starts_iri(input: &str, index: usize) -> bool {
    let tail = &input[index + 1..];
    match (tail.find('>'), tail.find(char::is_whitespace)) {
        (Some(end), Some(space)) => end < space,
        (Some(_), None) => true,
        _ => false,
    }
}

fn parse_modifiers(
    tail: &str,
) -> Result<
    (
        Vec<GroupBy>,
        Vec<Expression>,
        Vec<OrderByTerm>,
        Option<usize>,
        Option<usize>,
    ),
    RuntimeError,
> {
    let mut tail = tail.trim();
    let mut group_by = Vec::new();
    if tail.to_ascii_uppercase().starts_with("GROUP BY") {
        let rest = tail[8..].trim_start();
        let end = [
            keyword_position(rest, "HAVING"),
            keyword_position(rest, "ORDER BY"),
            keyword_position(rest, "OFFSET"),
            keyword_position(rest, "LIMIT"),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(rest.len());
        let mut group = rest[..end].trim();
        while !group.is_empty() {
            if group.starts_with('(') {
                let close = matching_paren(group, 0)?;
                let item = group[1..close].trim();
                let Some((expression, alias)) = item.rsplit_once(" AS ") else {
                    return Err(RuntimeError::UnsupportedSparql(
                        "GROUP BY expression 必须使用 `(expression AS ?variable)` 别名".into(),
                    ));
                };
                group_by.push(GroupBy::Expression {
                    expression: parse_expression(expression.trim())?,
                    variable: variable(alias.trim())?,
                });
                group = group[close + 1..].trim_start();
            } else {
                let token_end = group.find(char::is_whitespace).unwrap_or(group.len());
                group_by.push(GroupBy::Variable(variable(&group[..token_end])?));
                group = group[token_end..].trim_start();
            }
        }
        tail = rest[end..].trim_start();
    }
    let mut having = Vec::new();
    if tail.to_ascii_uppercase().starts_with("HAVING")
        && tail
            .as_bytes()
            .get("HAVING".len())
            .is_none_or(u8::is_ascii_whitespace)
    {
        let rest = tail["HAVING".len()..].trim_start();
        let end = [
            keyword_position(rest, "ORDER BY"),
            keyword_position(rest, "OFFSET"),
            keyword_position(rest, "LIMIT"),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(rest.len());
        let expressions = rest[..end].trim();
        let mut index = 0usize;
        while index < expressions.len() {
            while expressions
                .as_bytes()
                .get(index)
                .is_some_and(u8::is_ascii_whitespace)
            {
                index += 1;
            }
            if index == expressions.len() {
                break;
            }
            if expressions.as_bytes().get(index) != Some(&b'(') {
                return Err(RuntimeError::MalformedSparql(
                    "HAVING expression 必须包在 `()` 中".into(),
                ));
            }
            let close = matching_paren(expressions, index)?;
            having.push(parse_expression(expressions[index + 1..close].trim())?);
            index = close + 1;
        }
        tail = rest[end..].trim_start();
    }
    let modifier_at = [
        keyword_position(tail, "OFFSET"),
        keyword_position(tail, "LIMIT"),
    ]
    .into_iter()
    .flatten()
    .min()
    .unwrap_or(tail.len());
    let order_by = parse_order_by(tail[..modifier_at].trim())?;
    let mut rest = tail[modifier_at..].split_whitespace();
    let mut offset = None;
    let mut limit = None;
    while let Some(keyword) = rest.next() {
        let value = rest
            .next()
            .ok_or_else(|| RuntimeError::MalformedSparql(format!("{keyword} 必须是非负整数")))?;
        let value = value
            .parse::<usize>()
            .map_err(|_| RuntimeError::MalformedSparql(format!("{keyword} 必须是非负整数")))?;
        if keyword.eq_ignore_ascii_case("OFFSET") && offset.replace(value).is_none() {
            continue;
        }
        if keyword.eq_ignore_ascii_case("LIMIT") && limit.replace(value).is_none() {
            continue;
        }
        return Err(RuntimeError::UnsupportedSparql(
            "modifier 仅支持一次 OFFSET 和 LIMIT".into(),
        ));
    }
    Ok((group_by, having, order_by, offset, limit))
}

fn parse_order_by(tail: &str) -> Result<Vec<OrderByTerm>, RuntimeError> {
    if tail.is_empty() {
        return Ok(Vec::new());
    }
    let prefix = tail.trim_start();
    if !prefix.to_ascii_uppercase().starts_with("ORDER BY")
        || prefix
            .as_bytes()
            .get("ORDER BY".len())
            .is_some_and(|byte| !byte.is_ascii_whitespace())
    {
        return Err(RuntimeError::UnsupportedSparql(
            "目前仅支持 ORDER BY ?variable、ASC(?variable) 与 DESC(?variable)".into(),
        ));
    }
    split_order_terms(prefix["ORDER BY".len()..].trim())
        .into_iter()
        .map(|term| {
            let upper = term.to_ascii_uppercase();
            let (value, descending) = if upper.starts_with("ASC(") && term.ends_with(')') {
                (&term[4..term.len() - 1], false)
            } else if upper.starts_with("DESC(") && term.ends_with(')') {
                (&term[5..term.len() - 1], true)
            } else {
                (term, false)
            };
            Ok(OrderByTerm {
                expression: parse_expression(value.trim())?,
                descending,
            })
        })
        .collect()
}

/// ORDER BY term 可是带空格的算术/函数 expression，不能以 split_whitespace 切分。
fn split_order_terms(input: &str) -> Vec<&str> {
    let mut terms = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quoted = false;
    let mut iri = false;
    for (index, character) in input.char_indices() {
        match character {
            '\'' | '"' => quoted = !quoted,
            '<' if !quoted && starts_iri(input, index) => iri = true,
            '>' if iri => iri = false,
            '(' if !quoted && !iri => depth += 1,
            ')' if !quoted && !iri => depth = depth.saturating_sub(1),
            character if character.is_ascii_whitespace() && !quoted && !iri && depth == 0 => {
                if !input[start..index].trim().is_empty() {
                    terms.push(input[start..index].trim());
                }
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    if !input[start..].trim().is_empty() {
        terms.push(input[start..].trim());
    }
    terms
}

fn extract_filters(input: &str) -> Result<(String, Vec<Filter>), RuntimeError> {
    let upper = input.to_ascii_uppercase();
    let Some(start) = upper.find("FILTER") else {
        return Ok((input.into(), Vec::new()));
    };
    let mut depth = 0usize;
    let mut end = None;
    for (offset, character) in input[start + "FILTER".len()..].char_indices() {
        match character {
            '(' => depth += 1,
            ')' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    end = Some(start + "FILTER".len() + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end.ok_or_else(|| RuntimeError::MalformedSparql("FILTER 缺少 `)`".into()))?;
    let expression = input[start + "FILTER".len()..=end].trim();
    let compact = expression
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .collect::<String>();
    let filter = compact
        .strip_prefix("(lang(?")
        .and_then(|value| value.split_once(")='"))
        .and_then(|(variable, value)| {
            value
                .strip_suffix("')")
                .map(|language| (variable, language))
        })
        .map(|(variable, language)| Filter::Language {
            variable: variable.into(),
            language: language.into(),
        })
        .or_else(|| parse_equality_filter(expression).ok())
        .or_else(|| {
            parse_expression(expression.trim())
                .ok()
                .map(Filter::Expression)
        })
        .ok_or_else(|| {
            RuntimeError::UnsupportedSparql(
                "目前仅支持 FILTER 的 lang() 或 ?variable = 字面量/数值等值比较".into(),
            )
        })?;
    Ok((
        format!("{}{}", &input[..start], &input[end + 1..]),
        vec![filter],
    ))
}

fn parse_equality_filter(input: &str) -> Result<Filter, RuntimeError> {
    let expression = input.trim().trim_matches(['(', ')']).trim();
    let (left, right) = expression
        .split_once('=')
        .ok_or_else(|| RuntimeError::MalformedSparql("FILTER 缺少 =".into()))?;
    // 这个快速路径仅处理裸 `?variable = literal`。若左侧以 !、<、> 结尾，
    // 它是 !=、<=、>= 的一部分，必须交给通用 expression parser；否则会把
    // `?number >= 3` 误解为变量名 `number >` 的等值过滤。
    if matches!(left.trim_end().chars().next_back(), Some('!' | '<' | '>')) {
        return Err(RuntimeError::UnsupportedSparql(
            "复合比较应由 expression parser 处理".into(),
        ));
    }
    let bare_variable = left.trim();
    if !bare_variable.starts_with('?')
        || !bare_variable[1..]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(RuntimeError::UnsupportedSparql(
            "非裸变量等值比较应由 expression parser 处理".into(),
        ));
    }
    let variable = variable(bare_variable)?;
    let right = right.trim();
    if let Ok(number) = right.parse::<f64>() {
        return Ok(Filter::Equal {
            variable,
            value: FilterValue::Numeric(number),
        });
    }
    let (value, suffix) = quoted_literal(right)?;
    let datatype = suffix
        .strip_prefix("^^<")
        .and_then(|value| value.strip_suffix('>'))
        .map(str::to_owned);
    if !suffix.is_empty() && datatype.is_none() {
        return Err(RuntimeError::UnsupportedSparql(
            "FILTER literal 仅支持 ^^<IRI> datatype".into(),
        ));
    }
    Ok(Filter::Equal {
        variable,
        value: FilterValue::Literal { value, datatype },
    })
}

fn quoted_literal(input: &str) -> Result<(String, &str), RuntimeError> {
    let Some(rest) = input.strip_prefix('"') else {
        return Err(RuntimeError::UnsupportedSparql(
            "FILTER 右侧必须是 RDF literal 或数值".into(),
        ));
    };
    let mut escaped = false;
    for (index, character) in rest.char_indices() {
        if character == '"' && !escaped {
            return Ok((rest[..index].replace("\\\"", "\""), &rest[index + 1..]));
        }
        escaped = character == '\\' && !escaped;
        if character != '\\' {
            escaped = false;
        }
    }
    Err(RuntimeError::MalformedSparql(
        "FILTER literal 缺少结束引号".into(),
    ))
}

/// 移除开头的 PREFIX 声明，并只在 IRI/literal 之外展开 prefixed name。
fn expand_prefixes(input: &str) -> Result<String, RuntimeError> {
    let mut prefixes = BTreeMap::new();
    // SPARQL 1.1 的常用实现为 xsd/rdf 等内建前缀提供预声明；Ontop 基线的 datatype
    // query 即依赖未显式写出 PREFIX xsd 的这种约定。
    prefixes.insert("xsd:".into(), "http://www.w3.org/2001/XMLSchema#".into());
    prefixes.insert(
        "rdf:".into(),
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#".into(),
    );
    prefixes.insert(
        "rdfs:".into(),
        "http://www.w3.org/2000/01/rdf-schema#".into(),
    );
    let mut remaining = input.trim_start();
    loop {
        // 固定 LUBM query 在 PREFIX 前有说明注释；它们不能阻止后续声明进入
        // prefix 表，且不属于要交给语法解析器的 query body。
        if remaining.starts_with('#') {
            let line_end = remaining.find('\n').unwrap_or(remaining.len());
            remaining = remaining[line_end..]
                .trim_start_matches(['\r', '\n'])
                .trim_start();
            continue;
        }
        let upper = remaining.get(..6).map(str::to_ascii_uppercase);
        if upper.as_deref() != Some("PREFIX")
            || remaining
                .as_bytes()
                .get(6)
                .is_some_and(|byte| !byte.is_ascii_whitespace())
        {
            break;
        }
        let line_end = remaining.find('\n').unwrap_or(remaining.len());
        let declaration = &remaining[..line_end];
        let terms = declaration.split_whitespace().collect::<Vec<_>>();
        let valid_prefix = terms.get(1).is_some_and(|prefix| {
            prefix.strip_suffix(':').is_some_and(|label| {
                label.is_empty()
                    || label
                        .chars()
                        .next()
                        .is_some_and(|character| character.is_alphabetic() || character == '_')
                        && label.chars().all(|character| {
                            character.is_alphanumeric() || matches!(character, '_' | '-' | '.')
                        })
            })
        });
        if terms.len() != 3
            || !terms[1].ends_with(':')
            || !valid_prefix
            || !terms[2].starts_with('<')
            || !terms[2].ends_with('>')
        {
            return Err(RuntimeError::MalformedSparql("无效 PREFIX 声明".into()));
        }
        prefixes.insert(
            terms[1].to_owned(),
            terms[2][1..terms[2].len() - 1].to_owned(),
        );
        remaining = remaining[line_end..]
            .trim_start_matches(['\r', '\n'])
            .trim_start();
    }
    if prefixes.is_empty() {
        return Ok(remaining.into());
    }
    let mut output = String::with_capacity(remaining.len());
    let bytes = remaining.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'<' => {
                let end = remaining[index..].find('>').map(|offset| index + offset);
                if let Some(end) =
                    end.filter(|end| !remaining[index + 1..*end].chars().any(char::is_whitespace))
                {
                    output.push_str(&remaining[index..=end]);
                    index = end + 1;
                } else {
                    // `<` 也可能是 expression 的关系比较运算符，而不是 IRI 起始符。
                    output.push('<');
                    index += 1;
                }
            }
            b'"' => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && (bytes[index] != b'"' || bytes[index.saturating_sub(1)] == b'\\')
                {
                    index += 1;
                }
                if index == bytes.len() {
                    return Err(RuntimeError::MalformedSparql("literal 缺少结束引号".into()));
                }
                index += 1;
                output.push_str(&remaining[start..index]);
                if bytes.get(index..index + 2) == Some(b"^^") {
                    index += 2;
                    output.push_str("^^");
                    if bytes.get(index) == Some(&b'<') {
                        let end = remaining[index + 1..]
                            .find('>')
                            .map(|offset| index + offset + 1)
                            .ok_or_else(|| {
                                RuntimeError::MalformedSparql("literal datatype 缺少 `>`".into())
                            })?;
                        output.push_str(&remaining[index..=end]);
                        index = end + 1;
                    } else {
                        let datatype_start = index;
                        while index < bytes.len()
                            && !bytes[index].is_ascii_whitespace()
                            && !matches!(bytes[index], b'{' | b'}' | b'.' | b';' | b',' | b')')
                        {
                            index += 1;
                        }
                        let datatype = &remaining[datatype_start..index];
                        let (prefix, local) = datatype.split_once(':').ok_or_else(|| {
                            RuntimeError::MalformedSparql(
                                "literal datatype 必须是 IRI 或 prefixed name".into(),
                            )
                        })?;
                        let key = format!("{prefix}:");
                        let iri = prefixes.get(&key).ok_or_else(|| {
                            RuntimeError::MalformedSparql(format!("未声明的 PREFIX：{key}"))
                        })?;
                        output.push('<');
                        output.push_str(iri);
                        output.push_str(local);
                        output.push('>');
                    }
                }
            }
            byte if byte.is_ascii_whitespace()
                || matches!(byte, b'{' | b'}' | b'.' | b';' | b'(' | b')' | b',' | b'=') =>
            {
                output.push(byte as char);
                index += 1;
            }
            _ => {
                let start = index;
                while index < bytes.len()
                    && !bytes[index].is_ascii_whitespace()
                    && (!matches!(
                        bytes[index],
                        b'{' | b'}' | b'.' | b';' | b'(' | b')' | b',' | b'='
                    ) || (bytes[index] == b'.'
                        && ((index > start && bytes[index - 1] == b'\\')
                            || (remaining[start..index].contains(':')
                                && bytes.get(index + 1).is_some_and(|byte| {
                                    byte.is_ascii_alphanumeric() || *byte == b'_'
                                }))
                            // PREFIX label 本身可以包含 `.`（例如 `x.y:`）；在
                            // 看到其末尾的 `:` 前不能把中间点当作 triple delimiter。
                            || remaining[index + 1..]
                                .find(':')
                                .is_some_and(|offset| {
                                    remaining[index + 1..index + 1 + offset].chars().all(
                                        |character| {
                                            character.is_ascii_alphanumeric()
                                                || matches!(character, '_' | '-' | '.')
                                        },
                                    )
                                }))))
                {
                    index += 1;
                }
                let token = &remaining[start..index];
                if token == "a" {
                    output.push_str(RDF_TYPE);
                } else if bytes.get(index) == Some(&b'(') {
                    // `ofn:daysBetween(...)` 之类的函数名不是 RDF term，不能展开成 IRI。
                    output.push_str(token);
                } else {
                    output.push_str(&expand_prefixed_path_token(token, &prefixes)?);
                }
            }
        }
    }
    Ok(output)
}

/// path token 内可能并列多个 prefixed name（`ex:p1/ex:p2`、`^ex:p`）。逐个
/// 展开而非对第一个 `:` 做 split，确保 path operator 不会被吞进 local name。
fn expand_prefixed_path_token(
    token: &str,
    prefixes: &BTreeMap<String, String>,
) -> Result<String, RuntimeError> {
    // 不允许把冒号转义进 local name，也不能让 `?x:a`、`_:x:y` 或
    // `:a:b` 这类多个冒号的 token 被正则从中间截取为貌似合法的 prefixed
    // name。DAWG syn-bad-pname-06..13 要求在这里拒绝，而不是生成损坏 AST。
    let colon_count = token.matches(':').count();
    if token.contains("\\:")
        || (token.starts_with('?') && colon_count > 0)
        || (token.starts_with("_:") && colon_count > 1)
        || (colon_count > 2 && !token.contains(['/', '|', '^']))
    {
        return Err(RuntimeError::MalformedSparql("无效 prefixed name".into()));
    }
    if token.starts_with("_:") {
        return Ok(token.into());
    }
    // Unicode prefix/local name（例如食:食べる）不适合 ASCII 正则。无 path
    // operator 且无第二个冒号时，可直接按已声明 prefix 展开；复杂 path 仍由
    // 后续严格的既有解析器处理。
    if !token.contains(['/', '|', '^', '!', '*', '+']) {
        if let Some((prefix, local)) = token.split_once(':') {
            let key = format!("{prefix}:");
            if !local.contains(':') {
                if let Some(iri) = prefixes.get(&key) {
                    return Ok(format!("<{iri}{local}>"));
                }
            }
        }
    }
    // PNAME_LN 的 `\\?`、`\\~`、`\\.` 是 lexical escape，不是 token 边界。
    // 在取得 local name 前解码，避免把已展开 IRI 后残留的反斜杠误作新 RDF term。
    let normalized = token
        .replace("\\?", "?")
        .replace("\\~", "~")
        .replace("\\.", ".");
    let token = normalized.as_str();
    let names = Regex::new(
        r"(?P<prefix>[A-Za-z_][A-Za-z0-9_-]*|):(?P<local>:(?:[A-Za-z0-9_~%?.-]+)?|[A-Za-z0-9_~%?.-]+(?::[A-Za-z0-9_~%?.-]+)?)",
    )
    .expect("prefixed path name 正则固定有效");
    let mut output = String::with_capacity(token.len());
    let mut end = 0usize;
    for capture in names.captures_iter(token) {
        let matched = capture.get(0).expect("regex match");
        output.push_str(&token[end..matched.start()]);
        let key = format!("{}:", &capture["prefix"]);
        let iri = prefixes
            .get(&key)
            .ok_or_else(|| RuntimeError::MalformedSparql(format!("未声明的 PREFIX：{key}")))?;
        output.push('<');
        output.push_str(iri);
        output.push_str(&capture["local"]);
        output.push('>');
        end = matched.end();
    }
    if end == 0 {
        return Ok(token.into());
    }
    output.push_str(&token[end..]);
    Ok(output)
}

fn variable(token: &str) -> Result<String, RuntimeError> {
    token
        .strip_prefix('?')
        .map(str::to_owned)
        .ok_or_else(|| RuntimeError::MalformedSparql("SELECT 变量必须以 ? 开头".into()))
}

fn pattern_variables(pattern: &TriplePattern) -> Vec<String> {
    [
        pattern.subject.as_str(),
        pattern.predicate.as_str(),
        pattern.object.as_str(),
    ]
    .into_iter()
    .filter_map(|value| value.strip_prefix('?').map(str::to_owned))
    .collect()
}

fn group(input: &str) -> Result<Vec<TriplePattern>, RuntimeError> {
    let open = input
        .find('{')
        .ok_or_else(|| RuntimeError::MalformedSparql("缺少 `{`".into()))?;
    let close = input
        .rfind('}')
        .ok_or_else(|| RuntimeError::MalformedSparql("缺少 `}`".into()))?;
    if close <= open {
        return Err(RuntimeError::MalformedSparql("图模式分隔符为空".into()));
    }
    let terms = tokens(&input[open + 1..close])?;
    if terms.iter().all(|term| matches!(*term, "{" | "}")) {
        return Ok(Vec::new());
    }
    let (terms, graph) = if terms
        .first()
        .is_some_and(|term| term.eq_ignore_ascii_case("GRAPH"))
    {
        if terms.len() < 5 || terms[2] != "{" || terms.last() != Some(&"}") {
            return Err(RuntimeError::UnsupportedSparql(
                "GRAPH 必须包含一个具名图和完整三元组模式".into(),
            ));
        }
        let graph = terms[1];
        if (!graph.starts_with('<') || !graph.ends_with('>')) && !graph.starts_with('?') {
            return Err(RuntimeError::UnsupportedSparql(
                "GRAPH 仅支持 IRI 或变量具名图".into(),
            ));
        }
        let inner = terms[3..terms.len() - 1].to_vec();
        (
            inner,
            Some(
                graph
                    .strip_prefix('?')
                    .map(|name| format!("?{name}"))
                    .unwrap_or_else(|| graph.trim_matches(['<', '>']).to_owned()),
            ),
        )
    } else {
        (terms, None)
    };
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let triples = parse_triples(&terms)?;
    Ok(triples
        .into_iter()
        .map(|terms| TriplePattern {
            subject: terms[0].into(),
            predicate: terms[1].into(),
            object: terms[2].into(),
            graph: graph.clone(),
        })
        .collect())
}

fn parse_triples<'a>(terms: &[&'a str]) -> Result<Vec<[&'a str; 3]>, RuntimeError> {
    let mut triples = Vec::new();
    let mut index = 0;
    let mut subject = None;
    while index < terms.len() {
        // 完整 triple 后允许写成 `; .`；分号在此不再开启新的
        // predicate/object continuation（DAWG syntax-sparql3/syn-08）。
        if subject.is_some() && terms.get(index) == Some(&".") && index + 1 == terms.len() {
            subject = None;
            break;
        }
        let current_subject = subject.unwrap_or_else(|| terms[index]);
        if subject.is_none() {
            index += 1;
        }
        if index + 1 >= terms.len()
            || matches!(terms[index], ";" | ".")
            || matches!(terms[index + 1], ";" | ".")
        {
            return Err(RuntimeError::MalformedSparql(
                "图模式必须由完整三元组组成".into(),
            ));
        }
        if !valid_object_token(terms[index + 1]) {
            return Err(RuntimeError::MalformedSparql(
                "图模式 object 必须是变量、IRI 或合法 RDF literal".into(),
            ));
        }
        let current_predicate = terms[index];
        triples.push([current_subject, current_predicate, terms[index + 1]]);
        index += 2;
        match terms.get(index) {
            Some(&",") => {
                // comma continuation 复用当前 predicate；与 `;` 的区别是它不
                // 开启新的 predicate，而后续 `;` 仍可继续复用 subject。
                index += 1;
                loop {
                    let Some(object) = terms.get(index).copied() else {
                        return Err(RuntimeError::MalformedSparql(", 后缺少 object".into()));
                    };
                    if matches!(object, ";" | "." | ",") || !valid_object_token(object) {
                        return Err(RuntimeError::MalformedSparql(
                            ", 后必须是合法 RDF object".into(),
                        ));
                    }
                    triples.push([current_subject, current_predicate, object]);
                    index += 1;
                    match terms.get(index) {
                        Some(&",") => index += 1,
                        Some(&";") => {
                            subject = Some(current_subject);
                            index += 1;
                            break;
                        }
                        Some(&".") => {
                            subject = None;
                            index += 1;
                            break;
                        }
                        None => {
                            subject = None;
                            break;
                        }
                        Some(_) => {
                            return Err(RuntimeError::MalformedSparql(
                                "两个 object list 成员之间缺少 `,`、`.` 或 `;`".into(),
                            ));
                        }
                    }
                }
            }
            Some(&";") => {
                subject = Some(current_subject);
                index += 1;
            }
            Some(&".") => {
                subject = None;
                index += 1;
            }
            None => subject = None,
            Some(_) => {
                return Err(RuntimeError::MalformedSparql(
                    "两个三元组之间缺少 `.` 或 `;`".into(),
                ))
            }
        }
    }
    if subject.is_some() {
        return Err(RuntimeError::MalformedSparql("; 后缺少谓词对象".into()));
    }
    Ok(triples)
}

fn valid_object_token(token: &str) -> bool {
    token.starts_with('?')
        || token.starts_with('<') && token.ends_with('>')
        || token.starts_with("_:")
        || token.starts_with('"')
        || token.parse::<f64>().is_ok()
}

/// 将本阶段支持的 SPARQL 词法单元切开，同时完整保留含空格的 RDF literal。
fn tokens(input: &str) -> Result<Vec<&str>, RuntimeError> {
    let bytes = input.as_bytes();
    let mut result = Vec::new();
    let mut start = None;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b if b.is_ascii_whitespace() => {
                if let Some(start) = start.take() {
                    result.push(&input[start..index]);
                }
                index += 1;
            }
            // `.` 既是三元组分隔符，也是裸十进制数的组成部分。除了 `12.6`，
            // SPARQL 还允许末尾小数点的 `456.`；后者后面可以直接是 `}`，或再
            // 跟一个实际的三元组分隔点（`456. .`）。只在当前 token 已是数值
            // 前缀时把点并入 token，避免把 `ex:p.` 之类 IRI token 吞掉。
            b'.' if start.is_some()
                && bytes
                    .get(index.saturating_sub(1))
                    .is_some_and(u8::is_ascii_digit)
                && (bytes.get(index + 1).is_some_and(u8::is_ascii_digit)
                    || start.is_some_and(|start| {
                        !input[start..index].contains('.')
                            && input[start..index].parse::<f64>().is_ok()
                    })) =>
            {
                index += 1;
            }
            b'{' | b'}' | b'.' | b';' | b',' => {
                if let Some(start) = start.take() {
                    result.push(&input[start..index]);
                }
                result.push(&input[index..index + 1]);
                index += 1;
            }
            b'<' => {
                if start.is_some() {
                    // `!(<p1>|<p2>)` 的 negated property set 没有 whitespace；其
                    // `!`/`!(` 前缀与第一个 IRI 属于同一个 predicate token。
                    // 只为这两个语法前缀放行，普通 `token<iri>` 仍稳定报错。
                    if matches!(
                        start.map(|start| &input[start..index]),
                        Some("!") | Some("!(")
                    ) {
                        let path_start = start.take().expect("start 已检查存在");
                        let mut iri = false;
                        while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                            if matches!(bytes[index], b'.' | b';') && !iri {
                                break;
                            }
                            if bytes[index] == b'<' {
                                iri = true;
                            } else if bytes[index] == b'>' {
                                iri = false;
                            }
                            index += 1;
                        }
                        result.push(&input[path_start..index]);
                        continue;
                    }
                    return Err(RuntimeError::MalformedSparql("IRI 前缺少分隔符".into()));
                }
                let end = input[index + 1..]
                    .find('>')
                    .map(|offset| index + offset + 1)
                    .ok_or_else(|| RuntimeError::MalformedSparql("IRI 缺少 `>`".into()))?;
                // `<p>/<q>` 等 property path 必须作为一个 predicate token；逐个 IRI
                // 切分会让三元组 parser 把 `/` 误解为缺少分隔符。
                if input.as_bytes().get(end + 1).is_some_and(|byte| {
                    matches!(*byte, b'/' | b'|' | b'*' | b'?' | b'{')
                        // `<predicate>+11` 的 `+` 是紧邻 numeric object 的符号，
                        // 不可误判为 property-path one-or-more quantifier。
                        || (*byte == b'+'
                            && !input
                                .as_bytes()
                                .get(end + 2)
                                .is_some_and(u8::is_ascii_digit))
                }) {
                    let path_start = index;
                    index = end + 1;
                    let mut iri = false;
                    while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                        if matches!(bytes[index], b'.' | b';') && !iri {
                            break;
                        }
                        if bytes[index] == b'<' {
                            iri = true;
                        } else if bytes[index] == b'>' {
                            iri = false;
                        }
                        index += 1;
                    }
                    result.push(&input[path_start..index]);
                } else {
                    result.push(&input[index..=end]);
                    index = end + 1;
                }
            }
            b'"' => {
                if start.is_some() {
                    return Err(RuntimeError::MalformedSparql("literal 前缺少分隔符".into()));
                }
                let literal_start = index;
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'"' && bytes[index.saturating_sub(1)] != b'\\' {
                        index += 1;
                        break;
                    }
                    index += 1;
                }
                if index > bytes.len() || bytes.get(index - 1) != Some(&b'"') {
                    return Err(RuntimeError::MalformedSparql("literal 缺少结束引号".into()));
                }
                if bytes.get(index) == Some(&b'@') {
                    index += 1;
                    while bytes
                        .get(index)
                        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
                    {
                        index += 1;
                    }
                } else if bytes.get(index..index + 3) == Some(b"^^<") {
                    index += 3;
                    let end = input[index..]
                        .find('>')
                        .map(|offset| index + offset)
                        .ok_or_else(|| {
                            RuntimeError::MalformedSparql("literal datatype 缺少 `>`".into())
                        })?;
                    index = end + 1;
                }
                result.push(&input[literal_start..index]);
            }
            // inverse 或括号包围的 property path 可在 IRI 之前开始；将整个无空白
            // path token 保留，避免后面的 `<...>` 被误判为缺少分隔符。
            b'^' | b'(' if start.is_none() => {
                let path_start = index;
                let mut depth = 0usize;
                let mut iri = false;
                while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                    if matches!(bytes[index], b'.' | b';') && !iri && depth == 0 {
                        break;
                    }
                    match bytes[index] {
                        b'<' => iri = true,
                        b'>' => iri = false,
                        b'(' if !iri => depth += 1,
                        b')' if !iri => depth = depth.saturating_sub(1),
                        _ => {}
                    }
                    index += 1;
                }
                result.push(&input[path_start..index]);
            }
            _ => {
                start.get_or_insert(index);
                index += 1;
            }
        }
    }
    if let Some(start) = start {
        result.push(&input[start..]);
    }
    Ok(result)
}

/// 将单层 SPARQL collection 展开为 RDF first/rest 链。collection 的节点必须是
/// 查询局部变量，不能使用固定 blank-node label，否则会把 query 的 existential
/// blank node 错当成跨请求 identity。当前覆盖固定 DAWG basic/list 的非嵌套成员。
fn expand_collections(input: &str) -> Result<String, RuntimeError> {
    // 只从 group 开始识别 collection，不能把 SELECT projection `?x ?y
    // (?x + ?y AS ?z)`、VALUES tuple、函数调用或 IRI 内的 `.` 误当作 BGP。
    // 固定 basic/list 资产的 collection 均位于 group 起始位置。
    let pattern = Regex::new(r"(?s)(^|\{\s*)([^\s{};()]+)\s+([^\s{};()]+)\s+\(([^()]*)\)")
        .expect("collection regex 固定有效");
    let subject_pattern =
        Regex::new(r"(?ms)(^|[.{]\s*)\(\s*([^()]*)\s*\)\s+([^\s{};()]+)\s+([^\s{};()]+)")
            .expect("collection 主语正则固定有效");
    let mut index = 0usize;
    let mut error = None;
    let subject_expanded = subject_pattern.replace_all(input, |captures: &regex::Captures<'_>| {
        let members = match tokens(captures[2].trim()) {
            Ok(members) => members,
            Err(reason) => {
                error = Some(reason);
                return captures[0].to_owned();
            }
        };
        if members.is_empty() {
            return format!(
                "{}<http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> {} {}",
                &captures[1], &captures[3], &captures[4]
            );
        }
        let nodes = (0..members.len())
            .map(|_| {
                let node = format!("?__rtop_list_{index}");
                index += 1;
                node
            })
            .collect::<Vec<_>>();
        let mut output = format!(
            "{}{} {} {}",
            &captures[1], nodes[0], &captures[3], &captures[4]
        );
        for (member, node) in members.iter().zip(&nodes) {
            output.push_str(&format!(
                " . {node} <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> {member}"
            ));
        }
        for (position, node) in nodes.iter().enumerate() {
            let rest = nodes
                .get(position + 1)
                .map(String::as_str)
                .unwrap_or("<http://www.w3.org/1999/02/22-rdf-syntax-ns#nil>");
            output.push_str(&format!(
                " . {node} <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> {rest}"
            ));
        }
        output
    });
    if let Some(error) = error {
        return Err(error);
    }
    let expanded = pattern.replace_all(&subject_expanded, |captures: &regex::Captures<'_>| {
        if matches!(
            captures[2].to_ascii_uppercase().as_str(),
            "SELECT"
                | "ASK"
                | "CONSTRUCT"
                | "DESCRIBE"
                | "FILTER"
                | "BIND"
                | "VALUES"
                | "OPTIONAL"
                | "UNION"
                | "MINUS"
                | "GRAPH"
                | "WHERE"
        ) {
            return captures[0].to_owned();
        }
        let members = match tokens(captures[4].trim()) {
            Ok(members) => members,
            Err(reason) => {
                error = Some(reason);
                return captures[0].to_owned();
            }
        };
        if members.is_empty() {
            return format!(
                "{}{} {} <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil>",
                &captures[1], &captures[2], &captures[3]
            );
        }
        let nodes = (0..members.len())
            .map(|_| {
                let node = format!("?__rtop_list_{index}");
                index += 1;
                node
            })
            .collect::<Vec<_>>();
        let mut output = format!(
            "{}{} {} {}",
            &captures[1], &captures[2], &captures[3], nodes[0]
        );
        for (member, node) in members.iter().zip(&nodes) {
            output.push_str(&format!(
                " . {node} <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> {member}"
            ));
        }
        for (position, node) in nodes.iter().enumerate() {
            let rest = nodes
                .get(position + 1)
                .map(String::as_str)
                .unwrap_or("<http://www.w3.org/1999/02/22-rdf-syntax-ns#nil>");
            output.push_str(&format!(
                " . {node} <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> {rest}"
            ));
        }
        output
    });
    if let Some(error) = error {
        return Err(error);
    }
    let nested_standalone_pattern =
        Regex::new(r"(?ms)(^|[.{]\s*)\(\s*\(\s*([^()]*)\s*\)\s*\)\s*([.}])")
            .expect("嵌套独立 collection 正则固定有效");
    let nested_expanded = nested_standalone_pattern.replace_all(&expanded, |captures: &regex::Captures<'_>| {
        let outer = format!("?__rtop_list_{index}");
        index += 1;
        let mut output = captures[1].to_owned();
        let members = match tokens(captures[2].trim()) {
            Ok(members) => members,
            Err(reason) => {
                error = Some(reason);
                return captures[0].to_owned();
            }
        };
        if members.is_empty() {
            output.push_str(&format!(
                "{outer} <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> . {outer} <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil>{}",
                &captures[3]
            ));
            return output;
        }
        let nodes = (0..members.len())
            .map(|_| {
                let node = format!("?__rtop_list_{index}");
                index += 1;
                node
            })
            .collect::<Vec<_>>();
        output.push_str(&format!(
            "{outer} <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> {} . {outer} <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil>",
            nodes[0]
        ));
        for (position, (member, node)) in members.iter().zip(&nodes).enumerate() {
            let rest = nodes
                .get(position + 1)
                .map(String::as_str)
                .unwrap_or("<http://www.w3.org/1999/02/22-rdf-syntax-ns#nil>");
            output.push_str(&format!(
                " . {node} <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> {member} . {node} <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> {rest}"
            ));
        }
        output.push_str(&captures[3]);
        output
    });
    if let Some(error) = error {
        return Err(error);
    }
    let standalone_pattern = Regex::new(r"(?ms)(^|[.{]\s*)\(\s*([^()]*)\s*\)\s*([.}])")
        .expect("独立 collection 正则固定有效");
    let standalone_expanded = standalone_pattern.replace_all(&nested_expanded, |captures: &regex::Captures<'_>| {
        let start = captures.get(0).expect("regex 全匹配存在").start();
        let before = &nested_expanded[..start];
        if before
            .to_ascii_uppercase()
            .rfind("VALUES")
            .is_some_and(|values| values > before.rfind('}').unwrap_or(0))
        {
            // VALUES/BINDINGS 的 row tuple 复用圆括号，不是 collection。
            return captures[0].to_owned();
        }
        let members = match tokens(captures[2].trim()) {
            Ok(members) => members,
            Err(reason) => {
                error = Some(reason);
                return captures[0].to_owned();
            }
        };
        if members.is_empty() {
            // `()` 单独作为 graph pattern 不产生三元组，SPARQL grammar 因此拒绝；
            // 嵌套 collection 的 inner empty list 已由上面的专用规则处理。
            return captures[0].to_owned();
        }
        if members.iter().any(|member| member.eq_ignore_ascii_case("UNDEF")) {
            // VALUES tuple 的 lexical form 也以 `(…)` 表示；它不是 collection，
            // 留给 VALUES parser 处理每列的 UNDEF binding。
            return captures[0].to_owned();
        }
        let nodes = (0..members.len())
            .map(|_| {
                let node = format!("?__rtop_list_{index}");
                index += 1;
                node
            })
            .collect::<Vec<_>>();
        let mut output = captures[1].to_owned();
        for (position, (member, node)) in members.iter().zip(&nodes).enumerate() {
            if position > 0 {
                output.push_str(" . ");
            }
            output.push_str(&format!(
                "{node} <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> {member} . {node} <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> {}",
                nodes
                    .get(position + 1)
                    .map(String::as_str)
                    .unwrap_or("<http://www.w3.org/1999/02/22-rdf-syntax-ns#nil>")
            ));
        }
        output.push_str(&captures[3]);
        output
    });
    if let Some(error) = error {
        return Err(error);
    }
    Ok(standalone_expanded.into_owned())
}

/// 将对象位置的单层空白节点属性列表展开为两个普通三元组。
///
/// 例如 `?p :teaches [ :duration ?d ]` 等价于
/// `?p :teaches ?__rtop_blank_0 . ?__rtop_blank_0 :duration ?d`。展开在图模式
/// 解析之前完成，因此产生的变量自然参与 JOIN、FILTER 与子查询投影。
fn expand_blank_node_property_lists(input: &str) -> String {
    // collection 本身也可以是三元组的主语，且其成员可以是属性列表。这里在
    // collection 展开前一次性降低该组合形式，保留两个 list head、成员 blank
    // node 及其属性三元组的 query-local identity（DAWG syntax-forms-01）。
    let collection_subject_object_pattern = Regex::new(
        r"(?ms)(^|[.{]\s*)\(\s*\[\s*([^\s]+)\s+([^\s\]]+)\s*\]\s*\)\s+([^\s]+)\s+\(\s*\[\s*([^\s]+)\s+([^\s\]]+)\s*\]\s+([^\s)]+)\s*\)",
    )
    .expect("collection 主语/对象属性列表展开正则固定有效");
    // collection 内的 blank-node property list 必须先展开，再把 collection 降为
    // rdf:first/rdf:rest 链；否则 `(` 会把其中的 property path 与 triple token
    // 混在一起（DAWG syn-pp-in-collection）。这里的两个成员形式正是该固定
    // 语法资产，并保留成员 predicate 原样以交给 property-path 代数展开。
    let collection_pattern = Regex::new(
        r"(?s)(\?[A-Za-z_][A-Za-z0-9_]*|<[^>]+>)\s+([^\s]+)\s+\(\s*\[\s*([^\s]+)\s+([^\s\]]+)\s*\]\s+\[\s*([^\s]+)\s+([^\s\]]+)\s*\]\s*\)",
    )
    .expect("collection 内空白节点属性列表展开正则固定有效");
    // blank-node property list 也能处于 triple 的 subject 位置。先处理只在
    // group 开头或 `.` 后出现的这种形式，避免把 object 位置的 `[ p o ]`
    // 改写成缺少连接谓词的 token 序列（DAWG syntax-propertyPaths-01）。
    let subject_pattern = Regex::new(r"(?ms)(^|[.{]\s*)\[\s*([^\s]+)\s+([^\s\];]+)\s*;?\s*\]")
        .expect("subject 空白节点属性列表展开正则固定有效");
    let subject_object_pattern = Regex::new(r"(?ms)(^|[.{]\s*)\[\s*([^\s]+)\s+([^\s\]]+)\s*\]\s+([^\s]+)\s+\[\s*([^\s]+)\s+([^\s\]]+)\s*\]")
        .expect("subject/object 空白节点属性列表展开正则固定有效");
    let subject_two_semicolon_pattern = Regex::new(
        r"(?ms)(^|[.{]\s*)\[\s*([^\s]+)\s+([^\s;]+)\s*;\s*([^\s]+)\s+([^\s;]+)\s*;\s*([^\s]+)\s+([^\s\]]+)\s*\]",
    )
    .expect("带两个分号的 subject 空白节点属性列表展开正则固定有效");
    let subject_semicolon_pattern =
        Regex::new(r"(?ms)(^|[.{]\s*)\[\s*([^\s]+)\s+([^\s;]+)\s*;\s*([^\s]+)\s+([^\s\]]+)\s*\]")
            .expect("带分号的 subject 空白节点属性列表展开正则固定有效");
    let anonymous_subject_semicolon_pattern =
        Regex::new(r"(?ms)(^|[.{]\s*)\[\]\s+([^\s]+)\s+([^\s;]+)\s*;\s*([^\s]+)\s+([^\s\]]+)")
            .expect("带分号的匿名 subject 属性列表展开正则固定有效");
    let direct_pattern = Regex::new(
        r"(?s)(\?[A-Za-z_][A-Za-z0-9_]*|<[^>]+>)\s+([^\s]+)\s+\[\s*([^\s]+)\s+([^\s\]]+)\s*\]",
    )
    .expect("空白节点属性列表展开正则固定有效");
    let direct_semicolon_pattern = Regex::new(
        r"(?s)(\?[A-Za-z_][A-Za-z0-9_]*|<[^>]+>)\s+([^\s]+)\s+\[\s*([^\s]+)\s+([^\s;]+)\s*;\s*([^\s]+)\s+([^\s\]]+)\s*\]",
    )
    .expect("带分号的 object 空白节点属性列表展开正则固定有效");
    let semicolon_pattern = Regex::new(r"(?s);\s*([^\s]+)\s+\[\s*([^\s]+)\s+([^\s\]]+)\s*\]")
        .expect("分号后的空白节点属性列表展开正则固定有效");
    let mut index = 0usize;
    let collection_subject_object_expanded = collection_subject_object_pattern
        .replace_all(input, |captures: &regex::Captures<'_>| {
            let left_member = format!("?__rtop_blank_{index}");
            index += 1;
            let right_member = format!("?__rtop_blank_{index}");
            index += 1;
            let left_head = format!("?__rtop_list_{index}");
            index += 1;
            let right_head = format!("?__rtop_list_{index}");
            index += 1;
            let right_tail = format!("?__rtop_list_{index}");
            index += 1;
            format!(
                "{}{left_head} {} {right_head} . {left_head} <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> {left_member} . {left_head} <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> . {right_head} <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> {right_member} . {right_head} <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> {right_tail} . {right_tail} <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> {} . {right_tail} <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> . {left_member} {} {} . {right_member} {} {}",
                &captures[1],
                &captures[4],
                &captures[7],
                &captures[2],
                &captures[3],
                &captures[5],
                &captures[6],
            )
        })
        .into_owned();
    let collection_expanded = collection_pattern
        .replace_all(&collection_subject_object_expanded, |captures: &regex::Captures<'_>| {
            let first_member = format!("?__rtop_blank_{index}");
            index += 1;
            let second_member = format!("?__rtop_blank_{index}");
            index += 1;
            let head = format!("?__rtop_list_{index}");
            index += 1;
            let tail = format!("?__rtop_list_{index}");
            index += 1;
            format!(
                "{} {} {head} . {head} <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> {first_member} . {head} <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> {tail} . {tail} <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> {second_member} . {tail} <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> . {first_member} {} {} . {second_member} {} {}",
                &captures[1], &captures[2], &captures[3], &captures[4], &captures[5], &captures[6]
            )
        })
        .into_owned();
    let anonymous_subject_semicolon_expanded = anonymous_subject_semicolon_pattern
        .replace_all(&collection_expanded, |captures: &regex::Captures<'_>| {
            let blank = format!("?__rtop_blank_{index}");
            index += 1;
            format!(
                "{}{} {} {} . {blank} {} {}",
                &captures[1], blank, &captures[2], &captures[3], &captures[4], &captures[5]
            )
        })
        .into_owned();
    let subject_two_semicolon_expanded = subject_two_semicolon_pattern
        .replace_all(
            &anonymous_subject_semicolon_expanded,
            |captures: &regex::Captures<'_>| {
                let blank = format!("?__rtop_blank_{index}");
                index += 1;
                format!(
                    "{}{} {} {} . {blank} {} {} . {blank} {} {}",
                    &captures[1],
                    blank,
                    &captures[2],
                    &captures[3],
                    &captures[4],
                    &captures[5],
                    &captures[6],
                    &captures[7]
                )
            },
        )
        .into_owned();
    let subject_object_expanded = subject_object_pattern
        .replace_all(
            &subject_two_semicolon_expanded,
            |captures: &regex::Captures<'_>| {
                let subject_blank = format!("?__rtop_blank_{index}");
                index += 1;
                let object_blank = format!("?__rtop_blank_{index}");
                index += 1;
                format!(
                    "{}{} {} {} . {subject_blank} {} {object_blank} . {object_blank} {} {}",
                    &captures[1],
                    &subject_blank,
                    &captures[2],
                    &captures[3],
                    &captures[4],
                    &captures[5],
                    &captures[6]
                )
            },
        )
        .into_owned();
    let subject_semicolon_expanded = subject_semicolon_pattern
        .replace_all(
            &subject_object_expanded,
            |captures: &regex::Captures<'_>| {
                let blank = format!("?__rtop_blank_{index}");
                index += 1;
                format!(
                    "{}{} {} {} . {blank} {} {}",
                    &captures[1], blank, &captures[2], &captures[3], &captures[4], &captures[5]
                )
            },
        )
        .into_owned();
    let subject_expanded = subject_pattern
        .replace_all(
            &subject_semicolon_expanded,
            |captures: &regex::Captures<'_>| {
                let blank = format!("?__rtop_blank_{index}");
                index += 1;
                format!(
                    "{}{} {} {}",
                    &captures[1], blank, &captures[2], &captures[3]
                )
            },
        )
        .into_owned();
    let direct_semicolon_expanded = direct_semicolon_pattern
        .replace_all(&subject_expanded, |captures: &regex::Captures<'_>| {
            let blank = format!("?__rtop_blank_{index}");
            index += 1;
            format!(
                "{} {} {blank} . {blank} {} {} . {blank} {} {}",
                &captures[1], &captures[2], &captures[3], &captures[4], &captures[5], &captures[6]
            )
        })
        .into_owned();
    let expanded = direct_pattern
        .replace_all(
            &direct_semicolon_expanded,
            |captures: &regex::Captures<'_>| {
                let blank = format!("?__rtop_blank_{index}");
                index += 1;
                format!(
                    "{} {} {blank} . {blank} {} {}",
                    &captures[1], &captures[2], &captures[3], &captures[4]
                )
            },
        )
        .into_owned();
    semicolon_pattern
        .replace_all(&expanded, |captures: &regex::Captures<'_>| {
            let blank = format!("?__rtop_blank_{index}");
            index += 1;
            format!(
                "; {} {blank} . {blank} {} {}",
                &captures[1], &captures[2], &captures[3]
            )
        })
        .into_owned()
}

fn expand_anonymous_blank_nodes(input: &str) -> String {
    let mut index = 0usize;
    let pattern = Regex::new(r"\[\s*\]").expect("匿名 blank node 正则固定有效");
    pattern
        .replace_all(input, |_: &regex::Captures<'_>| {
            let blank = format!("?__rtop_anon_{index}");
            index += 1;
            blank
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        expand_anonymous_blank_nodes, expand_blank_node_property_lists, expand_prefixes,
        expression_variables, extract_filters, graph_pattern_variables, parse, parse_aggregate,
        parse_bind_at, parse_expression, parse_filter_at, parse_graph_pattern, parse_modifiers,
        parse_select_projection, parse_values_at, tokens, ComparisonOperator, Expression, Filter,
        GraphPattern, Query,
    };
    use crate::RuntimeError;

    #[test]
    fn exercises_expression_and_modifier_helper_boundaries() {
        assert!(
            parse_aggregate("GROUP_CONCAT(?name; SEPARATOR=\" / \")", "names")
                .unwrap()
                .is_some()
        );
        assert!(matches!(
            parse_expression("REPLACE(STR(?name), \"a\", \"b\")").unwrap(),
            Expression::Replace { .. }
        ));
        assert!(matches!(
            parse_expression("\"Ada\"@en").unwrap(),
            Expression::LanguageLiteral { .. }
        ));
        assert!(matches!(
            parse_expression("\"7\"^^<http://www.w3.org/2001/XMLSchema#integer>").unwrap(),
            Expression::TypedLiteral { .. }
        ));
        let (_, filters) = extract_filters("?s ?p ?o . FILTER(?o >= 3)").unwrap();
        assert!(matches!(filters.as_slice(), [Filter::Expression(_)]));
        let (group, having, order, offset, limit) = parse_modifiers(
            "GROUP BY ?name HAVING (COUNT(?name) > 1) ORDER BY DESC(?name) OFFSET 2 LIMIT 3",
        )
        .unwrap();
        assert_eq!(
            (group.len(), having.len(), order.len(), offset, limit),
            (1, 1, 1, Some(2), Some(3))
        );
        let expanded = expand_prefixes(
            "PREFIX ex: <https://example.test/>\nSELECT ?s WHERE { ?s ex:p/ex:q ?o }",
        )
        .unwrap();
        assert!(expanded.contains("<https://example.test/p>/<https://example.test/q>"));
        assert_eq!(
            parse_bind_at("BIND(STR(?name) AS ?label)", 0)
                .unwrap()
                .0
                .variable,
            "label"
        );
        assert!(matches!(
            parse_filter_at("FILTER(?age >= 18)", 0).unwrap().0,
            Filter::Expression(_)
        ));
        assert_eq!(
            parse_values_at("VALUES ?name { \"Ada\" }", 0)
                .unwrap()
                .0
                .len(),
            1
        );
        assert!(matches!(
            parse_graph_pattern("{ ?s <https://example.test/p> ?o }").unwrap(),
            GraphPattern::Bgp(_)
        ));
        assert!(matches!(
            parse_graph_pattern("{ SELECT ?s WHERE { ?s <https://example.test/p> ?o } }").unwrap(),
            GraphPattern::Subquery { .. }
        ));
        assert_eq!(
            parse_select_projection("DISTINCT ?name (COUNT(?name) AS ?count)")
                .unwrap()
                .3,
            true
        );
    }

    #[test]
    fn accepts_dollar_prefixed_variables_in_select_bgp_and_filter() {
        let query = parse(
            "SELECT $subject $object WHERE { $subject <https://example.test/p> $object . FILTER($object = $subject) }",
        )
        .expect("SPARQL 1.1 的 $variable 与 ?variable 等价");

        let Query::Select { variables, .. } = query else {
            panic!("应解析为 SELECT");
        };
        assert_eq!(variables, ["subject", "object"]);
    }

    #[test]
    fn classifies_query_form_and_describe_boundary_errors() {
        assert!(matches!(
            parse("SELECT ?value WHERE"),
            Err(RuntimeError::MalformedSparql(message)) if message == "缺少 `{`"
        ));
        assert!(matches!(
            parse("ASK { ?subject ?predicate ?object }"),
            Err(RuntimeError::NotFullyTranslatable(message)) if message.contains("谓词变量")
        ));
        assert!(matches!(
            parse("DESCRIBE literal"),
            Err(RuntimeError::MalformedSparql(message)) if message == "DESCRIBE target 必须是 IRI 或变量"
        ));
        assert!(matches!(
            parse("DESCRIBE  "),
            Err(RuntimeError::MalformedSparql(message)) if message == "DESCRIBE 缺少 target"
        ));
        assert!(matches!(
            parse("INSERT DATA { <https://example.test/s> <https://example.test/p> <https://example.test/o> }"),
            Err(RuntimeError::UnsupportedSparql(message)) if message.contains("仅支持")
        ));
    }

    #[test]
    fn preserves_supported_property_path_algebra_and_rejects_out_of_scope_forms() {
        // 固定 Ontop DAWG property-path 资产覆盖这些已承诺的代数形状；这里把
        // parser 层的可观察边界集中为稳定的正反例，避免静默扩大 path 范围。
        for query in [
            "SELECT ?object WHERE { <https://example.test/s> <https://example.test/p>/<https://example.test/q> ?object }",
            "SELECT ?object WHERE { <https://example.test/s> (<https://example.test/p>|<https://example.test/q>) ?object }",
            "SELECT ?object WHERE { <https://example.test/s> <https://example.test/p>{0} ?object }",
            "SELECT ?object WHERE { <https://example.test/s> ^<https://example.test/p> ?object }",
            "SELECT ?object WHERE { <https://example.test/s> <https://example.test/p>* ?object }",
        ] {
            assert!(parse(query).is_ok(), "已支持的 property path 被拒绝：{query}");
        }
        for query in [
            "SELECT ?object WHERE { <https://example.test/s> ?predicate* ?object }",
            "SELECT ?object WHERE { <https://example.test/s> ^?predicate ?object }",
            "SELECT ?object WHERE { <https://example.test/s> <https://example.test/p>+ ?object }",
            "SELECT ?object WHERE { <https://example.test/s> !(?predicate) ?object }",
        ] {
            assert!(
                matches!(parse(query), Err(RuntimeError::UnsupportedSparql(_))),
                "超出已验收范围的 property path 未被拒绝：{query}"
            );
        }
    }

    #[test]
    fn rejects_aggregate_projections_that_escape_group_scope() {
        assert!(matches!(
            parse(
                "SELECT ?name (COUNT(?name) AS ?count) WHERE { ?person <https://example.test/name> ?name } GROUP BY ?person",
            ),
            Err(RuntimeError::MalformedSparql(message)) if message.contains("?name 必须出现在 GROUP BY")
        ));
        let expression_scope = parse(
            "SELECT ?person (STR(?name) AS ?label) (COUNT(?name) AS ?count) WHERE { ?person <https://example.test/name> ?name } GROUP BY ?person",
        );
        assert!(
            matches!(
                expression_scope,
                Err(RuntimeError::MalformedSparql(ref message)) if message.contains("?label 必须出现在 GROUP BY")
            ),
            "{expression_scope:?}"
        );
        assert_eq!(
            expression_variables(&Expression::Function {
                name: "STR".into(),
                arguments: vec![Expression::Variable("name".into())],
                base_iri: None,
            }),
            vec!["name"],
        );
    }

    #[test]
    fn scopes_a_variable_named_graph_to_from_named_dataset_clauses() {
        let query = parse(
            "SELECT ?g ?s FROM NAMED <https://example.test/graph/a> FROM NAMED <https://example.test/graph/b> WHERE { GRAPH ?g { ?s <https://example.test/p> ?o } }",
        )
        .expect("FROM NAMED 的变量图模式应可解析");
        let Query::Select { pattern, .. } = query else {
            panic!("应解析为 SELECT");
        };
        let variables = graph_pattern_variables(&pattern);
        assert!(
            variables.iter().any(|variable| variable == "g"),
            "变量图绑定未出现在 graph_pattern_variables：{pattern:?}"
        );
        assert!(variables.iter().any(|variable| variable == "s"));
        assert!(variables.iter().any(|variable| variable == "o"));
        let GraphPattern::Scoped { pattern, .. } = pattern else {
            panic!("GRAPH 模式必须保留独立作用域");
        };
        assert!(matches!(*pattern, GraphPattern::Union(_, _)));
    }

    #[test]
    fn expands_a_blank_node_property_list_after_a_semicolon() {
        let expanded = expand_blank_node_property_lists(
            "?company :companyName ?name; :hasCompanyLocation [ a mo:Eastern_Asia ] .",
        );

        assert_eq!(
            expanded,
            "?company :companyName ?name; :hasCompanyLocation ?__rtop_blank_0 . ?__rtop_blank_0 a mo:Eastern_Asia ."
        );
    }

    #[test]
    fn expands_an_object_blank_node_property_list_with_two_members() {
        let expanded = expand_blank_node_property_lists("?x :p [ :v1 ?v1 ; :v2 ?v2 ] .");

        assert_eq!(
            expanded,
            "?x :p ?__rtop_blank_0 . ?__rtop_blank_0 :v1 ?v1 . ?__rtop_blank_0 :v2 ?v2 ."
        );
    }

    #[test]
    fn parses_dawg_construct_reification_property_list_asset() {
        assert!(parse(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/construct/query-reif-1.rq")).is_ok());
        assert!(parse(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/construct/query-reif-2.rq")).is_ok());
    }

    #[test]
    fn accepts_ontop_dawg_syntax_sparql5_reduced_assets() {
        for (asset, input) in [
            ("syntax-reduced-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql5/syntax-reduced-01.rq")),
            ("syntax-reduced-02.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql5/syntax-reduced-02.rq")),
        ] {
            assert!(parse(input).is_ok(), "固定 DAWG PositiveSyntaxTest {asset} 必须被接受");
        }
    }

    #[test]
    fn accepts_ontop_dawg_syntax_sparql1_manifest_assets() {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql1");
        let mut assets = std::fs::read_dir(&directory)
            .expect("固定 Ontop syntax-sparql1 基线目录必须可读")
            .map(|entry| entry.expect("目录项必须可读").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "rq"))
            .collect::<Vec<_>>();
        assets.sort();
        assert_eq!(assets.len(), 81, "manifest 的固定 syntax-sparql1 asset 数");
        for asset in assets {
            let input = std::fs::read_to_string(&asset).expect("syntax asset 必须可读");
            let parsed = parse(&input);
            assert!(
                parsed.is_ok(),
                "固定 DAWG PositiveSyntaxTest {} 必须被接受：{parsed:?}",
                asset.file_name().unwrap().to_string_lossy()
            );
        }
    }

    #[test]
    fn accepts_ontop_dawg_syntax_sparql2_manifest_assets() {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql2");
        let mut assets = std::fs::read_dir(&directory)
            .expect("固定 Ontop syntax-sparql2 基线目录必须可读")
            .map(|entry| entry.expect("目录项必须可读").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "rq"))
            .collect::<Vec<_>>();
        assets.sort();
        assert_eq!(assets.len(), 53, "manifest 的固定 syntax-sparql2 asset 数");
        for asset in assets {
            let input = std::fs::read_to_string(&asset).expect("syntax asset 必须可读");
            let parsed = parse(&input);
            assert!(
                parsed.is_ok(),
                "固定 DAWG PositiveSyntaxTest {} 必须被接受：{parsed:?}",
                asset.file_name().unwrap().to_string_lossy()
            );
        }
    }

    #[test]
    fn parses_optional_complex_2_graph_variable_asset() {
        let query = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/optional/q-opt-complex-2.rq");
        assert!(parse("SELECT ?name { ?person a <http://xmlns.com/foaf/0.1/Person>; <http://xmlns.com/foaf/0.1/name> ?name . }").is_ok());
        assert!(parse("SELECT ?name { GRAPH ?x { ?b <http://xmlns.com/foaf/0.1/name> ?name . ?b <http://xmlns.com/foaf/0.1/nick> ?nick } }").is_ok());
        assert!(parse("SELECT ?name { ?person a <http://xmlns.com/foaf/0.1/Person>; <http://xmlns.com/foaf/0.1/name> ?name . GRAPH ?x { ?b <http://xmlns.com/foaf/0.1/name> ?name . ?b <http://xmlns.com/foaf/0.1/nick> ?nick } }").is_ok());
        assert!(parse("SELECT ?id { ?person <http://example.org/things#empId> ?id OPTIONAL { { ?person <http://example.org/things#empId> ?id } UNION { ?person <http://example.org/things#ssn> ?ssn } } }").is_ok());
        parse(query).unwrap_or_else(|error| {
            panic!(
                "{error}: {}",
                expand_anonymous_blank_nodes(&expand_blank_node_property_lists(
                    &expand_prefixes(query).unwrap()
                ))
            )
        });
    }

    #[test]
    fn parses_a_prefixed_geosparql_bind_as_a_function_expression() {
        let query = parse(
            "PREFIX geo: <http://www.opengis.net/ont/geosparql#>\nPREFIX geof: <http://www.opengis.net/def/function/geosparql/>\nPREFIX uom: <http://www.opengis.net/def/uom/OGC/1.0/>\nSELECT ?v WHERE { ?x geo:asWKT ?wkt . BIND(geof:buffer(?wkt, 20, uom:metre) AS ?v) }",
        )
        .expect("GeoSPARQL BIND 应可解析");
        let Query::Select { pattern, .. } = query else {
            panic!("应解析为 SELECT");
        };
        let GraphPattern::Bind(_, binds) = pattern else {
            panic!("BIND 不应被忽略");
        };
        let Expression::Function {
            name, arguments, ..
        } = &binds[0].expression
        else {
            panic!("应为函数 expression：{:?}", binds[0].expression);
        };
        assert_eq!(name, "GEOF:BUFFER");
        assert_eq!(arguments.len(), 3);
    }

    #[test]
    fn parses_offset_and_limit_after_order_by() {
        let query = parse(
            "SELECT ?x WHERE { ?x <https://example.test/p> ?o } ORDER BY DESC(?x) OFFSET 2 LIMIT 3",
        )
        .expect("OFFSET 与 LIMIT 应作为 SELECT modifier 解析");
        let Query::Select {
            order_by,
            offset,
            limit,
            ..
        } = query
        else {
            panic!("应解析为 SELECT");
        };
        assert_eq!(order_by.len(), 1);
        assert_eq!(offset, Some(2));
        assert_eq!(limit, Some(3));
    }

    #[test]
    fn parses_having_and_expression_order_by_terms() {
        let query = parse(
            "SELECT ?category (SUM(?value) AS ?total) WHERE { ?entry <https://example.test/value> ?value ; <https://example.test/category> ?category } GROUP BY ?category HAVING (SUM(?value) > 10) ORDER BY DESC(SUM(?value)) (?value + 1)",
        )
        .expect("HAVING 与表达式 ORDER BY 应可解析");
        let Query::Select {
            having, order_by, ..
        } = query
        else {
            panic!("应解析为 SELECT");
        };
        assert_eq!(having.len(), 1);
        assert!(matches!(
            having.as_slice(),
            [Expression::Comparison {
                operator: ComparisonOperator::Greater,
                ..
            }]
        ));
        assert_eq!(order_by.len(), 2);
        assert!(order_by[0].descending);
        assert!(matches!(
            order_by[0].expression,
            Expression::Aggregate { .. }
        ));
        assert!(!order_by[1].descending);
        assert!(matches!(order_by[1].expression, Expression::Binary { .. }));
    }

    #[test]
    fn preserves_ontop_dawg_aggregate_projection_forms() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates",
        );
        for asset in [
            "agg-avg-02.rq",
            "agg-empty-group.rq",
            "agg-err-01.rq",
            "agg04.rq",
            "agg05.rq",
            "agg06.rq",
            "agg07.rq",
            "agg08b.rq",
            "agg-groupconcat-1.rq",
            "agg-groupconcat-2.rq",
            "agg-groupconcat-3.rq",
            "agg-max-02.rq",
            "agg-min-02.rq",
            "agg-sample-01.rq",
            "agg-sum-order-01.rq",
            "agg-err-02.rq",
        ] {
            let input = std::fs::read_to_string(root.join(asset))
                .expect("固定 Ontop DAWG aggregate asset 必须可读");
            assert!(parse(&input).is_ok(), "{asset} 应保留为可执行 AST");
        }

        let Query::Select {
            aggregates,
            projection_binds,
            group_by,
            order_by,
            ..
        } = parse(&std::fs::read_to_string(root.join("agg08b.rq")).unwrap())
            .expect("GROUP BY expression alias 应解析")
        else {
            panic!("agg08b 应为 SELECT");
        };
        assert_eq!(aggregates.len(), 1);
        assert!(projection_binds.is_empty());
        assert_eq!(group_by.len(), 1);
        assert_eq!(order_by.len(), 1);

        let Query::Ask { pattern } =
            parse(&std::fs::read_to_string(root.join("agg-groupconcat-3.rq")).unwrap())
                .expect("GROUP_CONCAT separator 子查询应解析")
        else {
            panic!("agg-groupconcat-3 应为 ASK");
        };
        assert!(matches!(pattern, GraphPattern::Filter(_, _)));
    }

    #[test]
    fn accepts_ontop_dawg_syntax_aggregate_manifest_assets() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query",
        );
        for number in 1..=15 {
            let asset = format!("syntax-aggregate-{number:02}.rq");
            let input = std::fs::read_to_string(root.join(&asset))
                .expect("固定 Ontop DAWG aggregate syntax asset 必须可读");
            let Query::Select { aggregates, .. } =
                parse(&input).unwrap_or_else(|error| panic!("{asset} 应形成 AST：{error}"))
            else {
                panic!("{asset} 应为 SELECT");
            };
            assert_eq!(
                aggregates.len(),
                1,
                "{asset} 必须保留一个 aggregate projection"
            );
        }
    }

    #[test]
    fn parses_prefixed_property_paths_and_correlated_filter_exists() {
        let query = parse(
            "PREFIX ex: <https://example.test/>\nSELECT ?person ?friend WHERE { ?person ex:knows/ex:knows ?friend . FILTER NOT EXISTS { ?friend ex:blocked ?blocked } }",
        )
        .expect("有界 prefixed property path 与 FILTER NOT EXISTS 应可解析");
        let Query::Select { pattern, .. } = query else {
            panic!("应解析为 SELECT");
        };
        let GraphPattern::Exists {
            pattern, negated, ..
        } = pattern
        else {
            panic!("应保留相关 EXISTS 图模式：{pattern:?}");
        };
        assert!(negated);
        assert!(matches!(*pattern, GraphPattern::Join(_, _)));

        for path in [
            "ex:knows|^ex:knownBy",
            "(ex:knows|ex:related)/ex:name",
            "ex:knows{2}",
        ] {
            parse(&format!(
                "PREFIX ex: <https://example.test/>\nSELECT ?value WHERE {{ ex:a {path} ?value }}"
            ))
            .unwrap_or_else(|error| panic!("property path {path} 应可解析：{error:?}"));
        }
    }

    #[test]
    fn accepts_dawg_syntax_query_exists_and_not_exists_forms() {
        let assets = [
            ("syntax-exists-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-exists-01.rq"), false),
            ("syntax-exists-02.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-exists-02.rq"), false),
            ("syntax-exists-03.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-exists-03.rq"), true),
            ("syntax-not-exists-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-not-exists-01.rq"), true),
            ("syntax-not-exists-02.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-not-exists-02.rq"), true),
            ("syntax-not-exists-03.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-not-exists-03.rq"), true),
        ];

        for (asset, input, negated) in assets {
            let Query::Select { pattern, .. } = parse(input).unwrap_or_else(|error| {
                panic!("固定 DAWG PositiveSyntaxTest11 {asset}：{error:?}")
            }) else {
                panic!("{asset} 应解析为 SELECT");
            };
            let GraphPattern::Exists {
                negated: actual, ..
            } = pattern
            else {
                panic!("{asset} 应保留 EXISTS 图模式：{pattern:?}");
            };
            assert_eq!(actual, negated, "{asset} 的 EXISTS 否定性");
        }
    }

    #[test]
    fn accepts_dawg_syntax_query_minus_oneof_bind_and_select_scope_forms() {
        let assets = [
            ("syntax-minus-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-minus-01.rq")),
            ("syntax-oneof-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-oneof-01.rq")),
            ("syntax-oneof-02.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-oneof-02.rq")),
            ("syntax-oneof-03.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-oneof-03.rq")),
            ("syntax-bind-02.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-bind-02.rq")),
            ("syntax-SELECTscope1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-SELECTscope1.rq")),
            ("syntax-SELECTscope3.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-SELECTscope3.rq")),
            ("syntax-propertyPaths-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-propertyPaths-01.rq")),
        ];

        for (asset, input) in assets {
            assert!(
                parse(input).is_ok(),
                "固定 DAWG PositiveSyntaxTest11 {asset} 应形成 AST"
            );
        }
    }

    #[test]
    fn accepts_dawg_syntax_query_construct_where_shorthand() {
        for (asset, input) in [
            ("syntax-construct-where-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-construct-where-01.rq")),
            ("syntax-construct-where-02.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-construct-where-02.rq")),
        ] {
            let Query::Construct { template, pattern } = parse(input)
                .unwrap_or_else(|error| panic!("固定 DAWG PositiveSyntaxTest11 {asset}：{error:?}"))
            else {
                panic!("{asset} 应解析为 CONSTRUCT");
            };
            assert_eq!(template.len(), 1, "{asset} 的简写 template");
            let GraphPattern::Bgp(patterns) = pattern else {
                panic!("{asset} 的简写 WHERE 应为 BGP")
            };
            assert_eq!(patterns.len(), 1, "{asset} 的 WHERE pattern");
            assert_eq!(template[0].object, "1816");
            assert_eq!(patterns[0].object, "1816");
        }
    }

    #[test]
    fn accepts_dawg_syntax_query_property_path_in_collection() {
        let query = parse(include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-pp-in-collection.rq"
        ))
        .expect("固定 DAWG PositiveSyntaxTest11 syn-pp-in-collection.rq 应形成 AST");
        let Query::Select { pattern, .. } = query else {
            panic!("syn-pp-in-collection.rq 应解析为 SELECT");
        };
        assert!(
            graph_pattern_variables(&pattern)
                .iter()
                .any(|variable| variable.starts_with("__rtop_list_")),
            "collection 应显式降为 RDF list 中间节点"
        );
    }

    #[test]
    fn preserves_ontop_dawg_collections_and_blank_node_property_lists() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for asset in ["list-1.rq", "list-2.rq", "list-3.rq", "list-4.rq"] {
            let input = std::fs::read_to_string(root.join(format!(
                "../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/{asset}"
            )))
            .expect("固定 Ontop DAWG basic/list asset 必须可读");
            let Query::Select { pattern, .. } =
                parse(&input).unwrap_or_else(|error| panic!("{asset} 应形成 AST：{error}"))
            else {
                panic!("{asset} 应为 SELECT");
            };
            assert!(
                matches!(pattern, GraphPattern::Bgp(_) | GraphPattern::Join(_, _)),
                "{asset} 的 collection 必须降为普通图模式：{pattern:?}"
            );
        }
        for asset in ["syntax-forms-01.rq", "syntax-forms-02.rq"] {
            let input = std::fs::read_to_string(root.join(format!(
                "../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql1/{asset}"
            )))
            .expect("固定 Ontop DAWG syntax form asset 必须可读");
            let Query::Select { pattern, .. } =
                parse(&input).unwrap_or_else(|error| panic!("{asset} 应形成 AST：{error}"))
            else {
                panic!("{asset} 应为 SELECT");
            };
            assert!(
                graph_pattern_variables(&pattern)
                    .iter()
                    .any(|variable| variable.starts_with("__rtop_blank_")
                        || variable.starts_with("__rtop_list_")),
                "{asset} 必须保留 query-local blank/list identity"
            );
        }
    }

    #[test]
    fn rejects_dawg_grouping_ungrouped_projection_assets() {
        for (asset, input) in [
            ("group06.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/grouping/group06.rq")),
            ("group07.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/grouping/group07.rq")),
        ] {
            assert!(
                matches!(
                    parse(input),
                    Err(RuntimeError::MalformedSparql(_) | RuntimeError::UnsupportedSparql(_))
                ),
                "固定 DAWG NegativeSyntaxTest11 {asset} 必须在 parser 边界拒绝"
            );
        }
    }

    #[test]
    fn applies_from_and_from_named_to_their_respective_graph_domains() {
        let default_query = parse(
            "SELECT ?person FROM <https://example.test/graph/default> WHERE { ?person <https://example.test/name> ?name }",
        )
        .expect("FROM 应作为默认图 dataset 解析");
        let Query::Select {
            pattern: GraphPattern::Bgp(patterns),
            ..
        } = default_query
        else {
            panic!("单一 FROM 应生成单一具名 BGP");
        };
        assert_eq!(
            patterns[0].graph.as_deref(),
            Some("https://example.test/graph/default")
        );

        let named_query = parse(
            "SELECT ?person FROM NAMED <https://example.test/graph/named> WHERE { GRAPH <https://example.test/graph/named> { ?person <https://example.test/name> ?name } }",
        )
        .expect("FROM NAMED 与 GRAPH 应可解析");
        assert!(matches!(named_query, Query::Select { .. }));

        let graph_variable = parse(
            "SELECT ?graph ?person WHERE { GRAPH ?graph { ?person <https://example.test/name> ?name } }",
        )
        .expect("GRAPH variable 应可解析");
        assert!(matches!(graph_variable, Query::Select { .. }));

        let service =
            parse("SELECT ?person { SERVICE <http://example.invalid/sparql> { ?person ?p ?o } }");
        assert!(
            matches!(service, Err(RuntimeError::NotFullyTranslatable(message)) if message.starts_with("SERVICE"))
        );
    }

    #[test]
    fn accepts_crlf_prefix_declarations_from_manifest_queries() {
        let query = parse(
            "PREFIX : <https://example.test/>\r\nSELECT ?x WHERE { ?x a :Thing } ORDER BY ?x",
        )
        .expect("CRLF 的 PREFIX 查询应与 LF 查询同样可解析");
        assert!(matches!(query, Query::Select { .. }));
    }

    #[test]
    fn accepts_lubm_default_prefix_after_comments() {
        let query = parse(include_str!(
            "../../ontop/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/lubm/query-1.rq"
        ))
        .expect("LUBM 的默认 PREFIX 和紧邻句点的 local name 应可解析");
        assert!(matches!(query, Query::Select { .. }));
    }

    #[test]
    fn accepts_prefix_declarations_after_leading_manifest_comments() {
        let query = parse(
            "# LUBM description\n# another leading comment\nPREFIX ub: <http://example.test/ub#>\nSELECT ?x WHERE { ?x a ub:Student }",
        )
        .expect("leading comments must not hide a subsequent PREFIX declaration");
        assert!(matches!(query, Query::Select { .. }));
    }

    #[test]
    fn parses_composite_filter_operators_as_expressions() {
        for (operator, expected) in [
            ("!=", ComparisonOperator::NotEqual),
            ("<=", ComparisonOperator::LessOrEqual),
            (">=", ComparisonOperator::GreaterOrEqual),
        ] {
            let query = parse(&format!(
                "SELECT $number WHERE {{ $number <https://example.test/p> ?value . FILTER($number {operator} 3) }}"
            ))
            .expect("复合比较应可解析");
            let Query::Select {
                pattern: GraphPattern::Filter(_, filters),
                ..
            } = query
            else {
                panic!("应保留 FILTER expression")
            };
            let [Filter::Expression(Expression::Comparison { operator, .. })] = filters.as_slice()
            else {
                panic!("应为 comparison expression：{filters:?}")
            };
            assert!(std::mem::discriminant(operator) == std::mem::discriminant(&expected));
        }
    }

    #[test]
    fn parses_ontop_values_tuple_with_an_unbound_cell() {
        let query = parse(include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bindings/values04.rq"
        ))
        .expect("SPARQL 1.1 VALUES tuple 的 UNDEF 应表示未绑定变量");
        assert!(matches!(query, Query::Select { .. }));

        let multiple_undef = parse(include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bindings/values05.rq"
        ))
        .expect("VALUES tuple 的每一列都可独立为 UNDEF");
        assert!(matches!(multiple_undef, Query::Select { .. }));
    }

    #[test]
    fn preserves_ontop_dawg_bindings_and_values_syntax_assets() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query",
        );
        for asset in [
            "syntax-bindings-01.rq",
            "syntax-bindings-02.rq",
            "syntax-bindings-02a.rq",
            "syntax-bindings-03.rq",
            "syntax-bindings-03a.rq",
            "syntax-bindings-04.rq",
            "syntax-bindings-05.rq",
            "syntax-bindings-05a.rq",
        ] {
            let query = std::fs::read_to_string(root.join(asset))
                .expect("固定 Ontop DAWG bindings asset 必须可读");
            assert!(parse(&query).is_ok(), "{asset} 应形成可执行 AST");
        }
        let malformed = std::fs::read_to_string(root.join("syntax-bindings-09.rq"))
            .expect("固定 Ontop DAWG malformed bindings asset 必须可读");
        assert!(matches!(
            parse(&malformed),
            Err(RuntimeError::MalformedSparql(message)) if message == "VALUES tuple 的值数量不匹配"
        ));
    }

    #[test]
    fn keeps_a_bare_decimal_as_one_graph_pattern_token() {
        assert_eq!(
            tokens("?x <https://example.test/p> 12.6 .").unwrap(),
            ["?x", "<https://example.test/p>", "12.6", "."]
        );
        assert_eq!(
            tokens("?x <https://example.test/p> 456. }").unwrap(),
            ["?x", "<https://example.test/p>", "456.", "}"]
        );
        assert_eq!(
            tokens("?x <https://example.test/p> 456. .").unwrap(),
            ["?x", "<https://example.test/p>", "456.", "."]
        );
    }

    #[test]
    fn classifies_empty_or_unsupported_query_forms_at_the_parser_boundary() {
        for (input, expected) in [
            ("SELECT { }", "SELECT 需要一个变量"),
            (
                "INSERT DATA { <https://example.test/s> <https://example.test/p> <https://example.test/o> }",
                "仅支持 SELECT、ASK、CONSTRUCT 与 DESCRIBE",
            ),
            (
                "SELECT ?value WHERE { VALUES ?value { bare } }",
                "VALUES 值必须为 IRI、literal 或数值",
            ),
        ] {
            assert!(
                matches!(parse(input), Err(RuntimeError::MalformedSparql(message)) if message == expected)
                    || matches!(parse(input), Err(RuntimeError::UnsupportedSparql(message)) if message == expected),
                "{input}"
            );
        }
        assert!(matches!(
            parse("DESCRIBE ?resource"),
            Ok(Query::Describe { resources, pattern: None }) if resources == ["?resource"]
        ));
    }

    #[test]
    fn rejects_ontop_aggregate_manifest_ungrouped_projections() {
        for asset in ["agg08.rq", "agg09.rq", "agg10.rq", "agg11.rq", "agg12.rq"] {
            let input = match asset {
                "agg08.rq" => include_str!(
                    "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg08.rq"
                ),
                "agg09.rq" => include_str!(
                    "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg09.rq"
                ),
                "agg10.rq" => include_str!(
                    "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg10.rq"
                ),
                "agg11.rq" => include_str!(
                    "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg11.rq"
                ),
                "agg12.rq" => include_str!(
                    "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg12.rq"
                ),
                _ => unreachable!(),
            };
            assert!(
                matches!(
                    parse(input),
                    Err(RuntimeError::MalformedSparql(_) | RuntimeError::UnsupportedSparql(_))
                ),
                "固定 DAWG NegativeSyntaxTest11 {asset} 必须被拒绝"
            );
        }
    }

    #[test]
    fn rejects_ontop_syntax_query_manifest_negative_core_assets() {
        for asset in [
            "syn-bad-01.rq",
            "syn-bad-02.rq",
            "syn-bad-03.rq",
            "syn-bad-04.rq",
            "syn-bad-05.rq",
            "syn-bad-06.rq",
            "syn-bad-07.rq",
            "syn-bad-08.rq",
            "syntax-bindings-09.rq",
            "syntax-BINDscope6.rq",
            "syntax-BINDscope7.rq",
            "syntax-BINDscope8.rq",
            "syntax-SELECTscope2.rq",
        ] {
            let input = match asset {
                "syn-bad-01.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-01.rq"),
                "syn-bad-02.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-02.rq"),
                "syn-bad-03.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-03.rq"),
                "syn-bad-04.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-04.rq"),
                "syn-bad-05.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-05.rq"),
                "syn-bad-06.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-06.rq"),
                "syn-bad-07.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-07.rq"),
                "syn-bad-08.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-08.rq"),
                "syntax-bindings-09.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-bindings-09.rq"),
                "syntax-BINDscope6.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-BINDscope6.rq"),
                "syntax-BINDscope7.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-BINDscope7.rq"),
                "syntax-BINDscope8.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-BINDscope8.rq"),
                "syntax-SELECTscope2.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-SELECTscope2.rq"),
                _ => unreachable!(),
            };
            assert!(
                matches!(
                    parse(input),
                    Err(RuntimeError::MalformedSparql(_) | RuntimeError::UnsupportedSparql(_))
                ),
                "固定 DAWG NegativeSyntaxTest11 {asset} 必须被拒绝"
            );
        }
    }

    #[test]
    fn rejects_ontop_syntax_query_manifest_bad_prefixed_names() {
        for (asset, input) in [
            ("syn-bad-pname-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-pname-01.rq")),
            ("syn-bad-pname-02.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-pname-02.rq")),
            ("syn-bad-pname-03.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-pname-03.rq")),
            ("syn-bad-pname-04.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-pname-04.rq")),
            ("syn-bad-pname-05.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-pname-05.rq")),
            ("syn-bad-pname-06.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-pname-06.rq")),
            ("syn-bad-pname-07.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-pname-07.rq")),
            ("syn-bad-pname-08.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-pname-08.rq")),
            ("syn-bad-pname-09.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-pname-09.rq")),
            ("syn-bad-pname-10.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-pname-10.rq")),
            ("syn-bad-pname-11.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-pname-11.rq")),
            ("syn-bad-pname-12.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-pname-12.rq")),
            ("syn-bad-pname-13.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-bad-pname-13.rq")),
        ] {
            assert!(
                matches!(
                    parse(input),
                    Err(RuntimeError::MalformedSparql(_) | RuntimeError::UnsupportedSparql(_))
                ),
                "固定 DAWG NegativeSyntaxTest11 {asset} 必须被拒绝"
            );
        }
    }

    #[test]
    fn rejects_ontop_dawg_syntax_sparql4_negative_assets() {
        for (asset, input) in [
            ("syn-bad-34.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql4/syn-bad-34.rq")),
            ("syn-bad-35.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql4/syn-bad-35.rq")),
            ("syn-bad-36.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql4/syn-bad-36.rq")),
            ("syn-bad-37.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql4/syn-bad-37.rq")),
            ("syn-bad-38.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql4/syn-bad-38.rq")),
            ("syn-bad-OPT-breaks-BGP.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql4/syn-bad-OPT-breaks-BGP.rq")),
            ("syn-bad-UNION-breaks-BGP.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql4/syn-bad-UNION-breaks-BGP.rq")),
            ("syn-bad-GRAPH-breaks-BGP.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql4/syn-bad-GRAPH-breaks-BGP.rq")),
        ] {
            assert!(
                matches!(
                    parse(input),
                    Err(RuntimeError::MalformedSparql(_) | RuntimeError::UnsupportedSparql(_))
                ),
                "固定 DAWG NegativeSyntaxTest {asset} 必须被拒绝"
            );
        }
    }

    #[test]
    fn accepts_ontop_dawg_syntax_sparql4_positive_assets() {
        for (asset, input) in [
            ("syn-09.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql4/syn-09.rq")),
            ("syn-10.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql4/syn-10.rq")),
            ("syn-11.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql4/syn-11.rq")),
            ("syn-leading-digits-in-prefixed-names.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql4/syn-leading-digits-in-prefixed-names.rq")),
        ] {
            assert!(
                parse(input).is_ok(),
                "固定 DAWG PositiveSyntaxTest {asset} 必须被接受：{:?}",
                parse(input)
            );
        }
    }

    #[test]
    fn rejects_ontop_dawg_syntax_sparql3_malformed_triple_blocks_01_08() {
        for (asset, input) in [
            ("syn-bad-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-01.rq")),
            ("syn-bad-02.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-02.rq")),
            ("syn-bad-03.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-03.rq")),
            ("syn-bad-04.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-04.rq")),
            ("syn-bad-05.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-05.rq")),
            ("syn-bad-06.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-06.rq")),
            ("syn-bad-07.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-07.rq")),
            ("syn-bad-08.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-08.rq")),
        ] {
            assert!(
                matches!(
                    parse(input),
                    Err(RuntimeError::MalformedSparql(_) | RuntimeError::UnsupportedSparql(_))
                ),
                "固定 DAWG NegativeSyntaxTest {asset} 必须被拒绝"
            );
        }
    }

    #[test]
    fn rejects_ontop_dawg_syntax_sparql3_malformed_triple_blocks_09_16() {
        for (asset, input) in [
            ("syn-bad-09.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-09.rq")),
            ("syn-bad-10.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-10.rq")),
            ("syn-bad-11.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-11.rq")),
            ("syn-bad-12.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-12.rq")),
            ("syn-bad-13.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-13.rq")),
            ("syn-bad-14.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-14.rq")),
            ("syn-bad-15.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-15.rq")),
            ("syn-bad-16.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-16.rq")),
        ] {
            assert!(
                matches!(
                    parse(input),
                    Err(RuntimeError::MalformedSparql(_) | RuntimeError::UnsupportedSparql(_))
                ),
                "固定 DAWG NegativeSyntaxTest {asset} 必须被拒绝"
            );
        }
    }

    #[test]
    fn rejects_ontop_dawg_syntax_sparql3_malformed_triple_blocks_17_24() {
        for (asset, input) in [
            ("syn-bad-17.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-17.rq")),
            ("syn-bad-18.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-18.rq")),
            ("syn-bad-19.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-19.rq")),
            ("syn-bad-20.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-20.rq")),
            ("syn-bad-21.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-21.rq")),
            ("syn-bad-22.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-22.rq")),
            ("syn-bad-23.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-23.rq")),
            ("syn-bad-24.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-24.rq")),
        ] {
            assert!(
                matches!(
                    parse(input),
                    Err(RuntimeError::MalformedSparql(_) | RuntimeError::UnsupportedSparql(_))
                ),
                "固定 DAWG NegativeSyntaxTest {asset} 必须被拒绝"
            );
        }
    }

    #[test]
    fn rejects_ontop_dawg_syntax_sparql3_malformed_terms_25_31() {
        for (asset, input) in [
            ("syn-bad-25.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-25.rq")),
            ("syn-bad-26.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-26.rq")),
            ("syn-bad-27.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-27.rq")),
            ("syn-bad-28.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-28.rq")),
            ("syn-bad-29.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-29.rq")),
            ("syn-bad-30.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-30.rq")),
            ("syn-bad-31.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-31.rq")),
        ] {
            assert!(
                matches!(
                    parse(input),
                    Err(RuntimeError::MalformedSparql(_) | RuntimeError::UnsupportedSparql(_))
                ),
                "固定 DAWG NegativeSyntaxTest {asset} 必须被拒绝"
            );
        }
    }

    #[test]
    fn rejects_ontop_dawg_syntax_sparql3_named_negative_assets() {
        for (asset, input) in [
            ("syn-bad-bnode-dot.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-bnode-dot.rq")),
            ("syn-bad-bnodes-missing-pvalues-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-bnodes-missing-pvalues-01.rq")),
            ("syn-bad-bnodes-missing-pvalues-02.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-bnodes-missing-pvalues-02.rq")),
            ("syn-bad-empty-optional-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-empty-optional-01.rq")),
            ("syn-bad-empty-optional-02.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-empty-optional-02.rq")),
            ("syn-bad-filter-missing-parens.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-filter-missing-parens.rq")),
            ("syn-bad-lone-list.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-lone-list.rq")),
            ("syn-bad-lone-node.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-bad-lone-node.rq")),
            ("syn-blabel-cross-graph-bad.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-blabel-cross-graph-bad.rq")),
            ("syn-blabel-cross-optional-bad.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-blabel-cross-optional-bad.rq")),
            ("syn-blabel-cross-union-bad.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-blabel-cross-union-bad.rq")),
        ] {
            assert!(
                matches!(
                    parse(input),
                    Err(RuntimeError::MalformedSparql(_) | RuntimeError::UnsupportedSparql(_))
                ),
                "固定 DAWG NegativeSyntaxTest {asset} 必须被拒绝"
            );
        }
    }

    #[test]
    fn accepts_ontop_dawg_syntax_sparql3_blank_node_across_filter() {
        assert!(parse(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-blabel-cross-filter.rq")).is_ok());
    }

    #[test]
    fn accepts_ontop_dawg_syntax_sparql3_positive_assets_01_08() {
        for (asset, input) in [
            ("syn-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-01.rq")),
            ("syn-02.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-02.rq")),
            ("syn-03.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-03.rq")),
            ("syn-04.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-04.rq")),
            ("syn-05.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-05.rq")),
            ("syn-06.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-06.rq")),
            ("syn-07.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-07.rq")),
            ("syn-08.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/syntax-sparql3/syn-08.rq")),
        ] {
            assert!(
                parse(input).is_ok(),
                "固定 DAWG PositiveSyntaxTest {asset} 必须被接受：{:?}",
                parse(input)
            );
        }
    }

    #[test]
    fn accepts_ontop_syntax_query_manifest_valid_prefixed_names() {
        for (asset, input) in [
            ("syn-pname-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-pname-01.rq")),
            ("syn-pname-02.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-pname-02.rq")),
            ("syn-pname-03.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-pname-03.rq")),
            ("syn-pname-04.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-pname-04.rq")),
            ("syn-pname-05.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-pname-05.rq")),
            ("syn-pname-06.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-pname-06.rq")),
            ("syn-pname-07.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-pname-07.rq")),
            ("syn-pname-08.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-pname-08.rq")),
            ("syn-pname-09.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syn-pname-09.rq")),
        ] {
            assert!(parse(input).is_ok(), "固定 DAWG PositiveSyntaxTest11 {asset}");
        }
    }

    #[test]
    fn accepts_ontop_syntax_query_manifest_select_expression_assets() {
        for (asset, input) in [
            ("syntax-select-expr-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-select-expr-01.rq")),
            ("syntax-select-expr-02.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-select-expr-02.rq")),
            ("syntax-select-expr-03.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-select-expr-03.rq")),
            ("syntax-select-expr-04.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-select-expr-04.rq")),
            ("syntax-select-expr-05.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-select-expr-05.rq")),
        ] {
            assert!(parse(input).is_ok(), "固定 DAWG PositiveSyntaxTest11 {asset}");
        }
    }

    #[test]
    fn accepts_ontop_syntax_query_manifest_subquery_assets() {
        for (asset, input) in [
            ("syntax-subquery-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-subquery-01.rq")),
            ("syntax-subquery-02.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-subquery-02.rq")),
            ("syntax-subquery-03.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-subquery-03.rq")),
        ] {
            assert!(parse(input).is_ok(), "固定 DAWG PositiveSyntaxTest11 {asset}");
        }
    }

    #[test]
    fn accepts_ontop_syntax_query_manifest_bind_scope_assets() {
        for (asset, input) in [
            ("syntax-BINDscope1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-BINDscope1.rq")),
            ("syntax-BINDscope2.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-BINDscope2.rq")),
            ("syntax-BINDscope3.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-BINDscope3.rq")),
            ("syntax-BINDscope4.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-BINDscope4.rq")),
            ("syntax-BINDscope5.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-BINDscope5.rq")),
        ] {
            assert!(parse(input).is_ok(), "固定 DAWG PositiveSyntaxTest11 {asset}");
        }
    }

    #[test]
    fn accepts_ontop_syntax_query_manifest_bindings_values_assets() {
        for (asset, input) in [
            ("syntax-bindings-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-bindings-01.rq")),
            ("syntax-bindings-02.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-bindings-02.rq")),
            ("syntax-bindings-02a.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-bindings-02a.rq")),
            ("syntax-bindings-03.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-bindings-03.rq")),
            ("syntax-bindings-03a.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-bindings-03a.rq")),
            ("syntax-bindings-04.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-bindings-04.rq")),
            ("syntax-bindings-05.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-bindings-05.rq")),
            ("syntax-bindings-05a.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-bindings-05a.rq")),
        ] {
            assert!(parse(input).is_ok(), "固定 DAWG PositiveSyntaxTest11 {asset}");
        }
    }

    #[test]
    fn accepts_ontop_syntax_query_manifest_aggregate_core_assets() {
        for (asset, input) in [
            ("syntax-aggregate-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-aggregate-01.rq")),
            ("syntax-aggregate-02.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-aggregate-02.rq")),
            ("syntax-aggregate-03.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-aggregate-03.rq")),
            ("syntax-aggregate-04.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-aggregate-04.rq")),
            ("syntax-aggregate-05.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-aggregate-05.rq")),
            ("syntax-aggregate-06.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-aggregate-06.rq")),
            ("syntax-aggregate-07.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-aggregate-07.rq")),
        ] {
            assert!(parse(input).is_ok(), "固定 DAWG PositiveSyntaxTest11 {asset}");
        }
    }

    #[test]
    fn accepts_ontop_syntax_query_manifest_aggregate_extended_assets() {
        for (asset, input) in [
            ("syntax-aggregate-08.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-aggregate-08.rq")),
            ("syntax-aggregate-09.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-aggregate-09.rq")),
            ("syntax-aggregate-10.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-aggregate-10.rq")),
            ("syntax-aggregate-11.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-aggregate-11.rq")),
            ("syntax-aggregate-12.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-aggregate-12.rq")),
            ("syntax-aggregate-13.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-aggregate-13.rq")),
            ("syntax-aggregate-14.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-aggregate-14.rq")),
            ("syntax-aggregate-15.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/syntax-query/syntax-aggregate-15.rq")),
        ] {
            assert!(parse(input).is_ok(), "固定 DAWG PositiveSyntaxTest11 {asset}");
        }
    }
}
