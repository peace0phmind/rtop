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

#[derive(Debug, Clone)]
pub enum Query {
    Select {
        variables: Vec<String>,
        pattern: GraphPattern,
        aggregates: Vec<Aggregate>,
        projection_binds: Vec<Bind>,
        group_by: Vec<String>,
        distinct: bool,
        order_by: Vec<OrderByTerm>,
        offset: Option<usize>,
        limit: Option<usize>,
    },
    Ask {
        patterns: Vec<TriplePattern>,
    },
    Construct {
        template: Vec<TriplePattern>,
        patterns: Vec<TriplePattern>,
    },
    Describe {
        resource: String,
    },
}

/// parser 到 runtime 的查询代数 seam。
#[derive(Debug, Clone)]
pub enum GraphPattern {
    Empty,
    Bgp(Vec<TriplePattern>),
    Join(Box<GraphPattern>, Box<GraphPattern>),
    LeftJoin(Box<GraphPattern>, Box<GraphPattern>),
    Minus(Box<GraphPattern>, Box<GraphPattern>),
    Union(Box<GraphPattern>, Box<GraphPattern>),
    Subquery {
        pattern: Box<GraphPattern>,
        variables: Vec<String>,
        aggregates: Vec<Aggregate>,
        projection_binds: Vec<Bind>,
        group_by: Vec<String>,
        distinct: bool,
        order_by: Vec<OrderByTerm>,
        offset: Option<usize>,
        limit: Option<usize>,
    },
    /// VALUES 的每项是一条 solution mapping；单变量和 tuple 形式共用同一表示。
    Values(Vec<Binding>),
    Bind(Box<GraphPattern>, Vec<Bind>),
    Filter(Box<GraphPattern>, Vec<Filter>),
}

#[derive(Debug, Clone)]
pub struct BindingValue {
    pub variable: String,
    pub value: RdfTerm,
}

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
    pub distinct: bool,
    pub separator: Option<String>,
    /// 聚合 expression error 时由 SELECT 投影的 COALESCE 使用的后备表达式。
    pub fallback: Option<Expression>,
}

#[derive(Debug, Clone, Copy)]
pub enum AggregateKind {
    Count,
    Sum,
    Avg,
    Min,
    Max,
    GroupConcat,
}

#[derive(Debug, Clone)]
pub enum Expression {
    Not(Box<Expression>),
    Variable(String),
    String(String),
    TypedLiteral {
        value: String,
        datatype: String,
    },
    Iri(String),
    Number(String),
    Aggregate {
        kind: AggregateKind,
        expression: Box<Expression>,
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
    pub variable: String,
    pub descending: bool,
}

pub fn parse(input: &str) -> Result<Query, RuntimeError> {
    // 固定 Ontop manifest 中既有 CRLF 的 .rq 文件；在词法解析前归一化行结束符，
    // 保持 PREFIX、注释和 token 边界与 LF 输入一致。
    let input = input.replace("\r\n", "\n").replace('\r', "\n");
    // SPARQL 1.1 将 `$name` 与 `?name` 定义为同一变量语法。先在词法层归一化，
    // 让投影、BGP、FILTER、BIND、GROUP BY 和 ORDER BY 共用既有的 `?` 解析路径，
    // 同时不改写 IRI 或 string literal 内的 `$`。
    let input = normalize_dollar_variables(&input);
    let (input, base_iri) = extract_base(&input)?;
    let input = expand_prefixes(&input)?;
    let input = normalize_bare_boolean_literals(&input);
    validate_typed_boolean_literals(&input)?;
    validate_typed_datetime_literals(&input)?;
    let input = expand_blank_node_property_lists(&input);
    let input = expand_anonymous_blank_nodes(&input);
    let compact = input.trim();
    let upper = compact.to_ascii_uppercase();
    if upper.starts_with("SELECT ") {
        let open = compact
            .find('{')
            .ok_or_else(|| RuntimeError::MalformedSparql("缺少 `{`".into()))?;
        let (selected, mut binds, aggregates, distinct) =
            parse_select_projection(compact[6..open].trim())?;
        let close = compact
            .rfind('}')
            .ok_or_else(|| RuntimeError::MalformedSparql("缺少 `}`".into()))?;
        let mut pattern = parse_graph_pattern(&compact[open..=close])?;
        set_pattern_base_iri(&mut pattern, base_iri.as_deref());
        for bind in &mut binds {
            set_expression_base_iri(&mut bind.expression, base_iri.as_deref());
        }
        let tail = compact[close + 1..].trim();
        let (group_by, order_by, offset, limit) = parse_modifiers(tail)?;
        let variables = if selected.is_none() {
            graph_pattern_variables(&pattern)
                .into_iter()
                .chain(binds.iter().map(|bind| bind.variable.clone()))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect()
        } else {
            selected.expect("non-* projection parsed into variables")
        };
        if variables.is_empty() {
            return Err(RuntimeError::MalformedSparql("SELECT 需要一个变量".into()));
        }
        return Ok(Query::Select {
            variables,
            pattern,
            aggregates,
            projection_binds: binds,
            group_by,
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
        let patterns = group(&compact[open..])?;
        if patterns
            .iter()
            .any(|pattern| pattern.predicate.starts_with('?'))
        {
            return Err(RuntimeError::UnsupportedSparql(
                "ASK 尚不支持谓词变量".into(),
            ));
        }
        return Ok(Query::Ask { patterns });
    }
    if upper.starts_with("DESCRIBE ") {
        let resource = compact[8..].trim();
        if resource.starts_with('<') && resource.ends_with('>') {
            return Ok(Query::Describe {
                resource: resource.trim_matches(['<', '>']).into(),
            });
        }
        return Err(RuntimeError::UnsupportedSparql(
            "DESCRIBE 仅支持一个 IRI".into(),
        ));
    }
    if upper.starts_with("CONSTRUCT ") {
        let where_at = upper
            .find("WHERE")
            .ok_or_else(|| RuntimeError::MalformedSparql("CONSTRUCT 缺少 WHERE".into()))?;
        let template = group(compact[9..where_at].trim())?;
        return Ok(Query::Construct {
            template,
            patterns: group(compact[where_at + 5..].trim())?,
        });
    }
    Err(RuntimeError::UnsupportedSparql(
        "仅支持 SELECT、ASK、CONSTRUCT 与 DESCRIBE".into(),
    ))
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
    parse_group_content(&input[open + 1..close])
}

fn parse_group_content(content: &str) -> Result<GraphPattern, RuntimeError> {
    let mut current = GraphPattern::Empty;
    let mut index = 0usize;
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
            (
                GraphPattern::Bgp(group(&format!("{{{}}}", &content[index..=close]))?),
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
                    distinct,
                    order_by,
                    offset,
                    limit,
                }
            } else {
                parse_group_content(inner)?
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
            index = close + 1;
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
            current = GraphPattern::Bind(Box::new(current), vec![bind]);
            index = next;
            continue;
        } else if starts_keyword_at(content, index, "FILTER") {
            let (filter, next) = parse_filter_at(content, index)?;
            current = GraphPattern::Filter(Box::new(current), vec![filter]);
            index = next;
            continue;
        } else if starts_keyword_at(content, index, "VALUES") {
            let (values, next) = parse_values_at(content, index)?;
            current = join_graph_patterns(current, GraphPattern::Values(values));
            index = next;
            continue;
        } else {
            let end = next_group_construct(content, index);
            if end == index {
                return Err(RuntimeError::UnsupportedSparql("无法解析图模式项".into()));
            }
            let patterns = group(&format!("{{{}}}", content[index..end].trim()))?;
            (GraphPattern::Bgp(patterns), end)
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
    Ok(current)
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
        .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'.')
    {
        index += 1;
    }
    index
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
            '{' if !quoted && !iri => return index,
            _ if !quoted
                && !iri
                && ["OPTIONAL", "MINUS", "BIND", "FILTER", "VALUES", "UNION"]
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

fn parse_values_at(input: &str, start: usize) -> Result<(Vec<Binding>, usize), RuntimeError> {
    let mut index = skip_group_separators(input, start + "VALUES".len());
    if input.as_bytes().get(index) == Some(&b'(') {
        let variables_close = matching_paren(input, index)?;
        let variables = tokens(&input[index + 1..variables_close])?
            .into_iter()
            .map(variable)
            .collect::<Result<Vec<_>, _>>()?;
        if variables.is_empty() {
            return Err(RuntimeError::MalformedSparql(
                "VALUES tuple 缺少变量".into(),
            ));
        }
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
                binding.insert(variable.clone(), value_term(term)?);
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

fn graph_pattern_variables(pattern: &GraphPattern) -> Vec<String> {
    match pattern {
        GraphPattern::Empty => Vec::new(),
        GraphPattern::Bgp(patterns) => patterns.iter().flat_map(pattern_variables).collect(),
        GraphPattern::Join(left, right)
        | GraphPattern::LeftJoin(left, right)
        | GraphPattern::Minus(left, right)
        | GraphPattern::Union(left, right) => {
            let mut variables = graph_pattern_variables(left);
            variables.extend(graph_pattern_variables(right));
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
        GraphPattern::Filter(pattern, _) => graph_pattern_variables(pattern),
    }
}

fn parse_subquery(input: &str) -> Result<Option<GraphPattern>, RuntimeError> {
    let open = input
        .find('{')
        .ok_or_else(|| RuntimeError::MalformedSparql("缺少 `{`".into()))?;
    let close = matching_brace(input, open)?;
    if !input[close + 1..].trim().is_empty() {
        return Ok(None);
    }
    let content = input[open + 1..close].trim();
    if !content.starts_with('{') {
        return Ok(None);
    }
    let nested_close = matching_brace(content, 0)?;
    if !content[nested_close + 1..].trim().is_empty() {
        return Ok(None);
    }
    let nested = content[1..nested_close].trim();
    if !nested.to_ascii_uppercase().starts_with("SELECT") {
        return Ok(None);
    }
    let Query::Select {
        variables,
        pattern,
        aggregates,
        projection_binds,
        group_by,
        distinct,
        order_by,
        offset,
        limit,
    } = parse(nested)?
    else {
        unreachable!("SELECT prefix was checked")
    };
    Ok(Some(GraphPattern::Subquery {
        pattern: Box::new(pattern),
        variables,
        aggregates,
        projection_binds,
        group_by,
        distinct,
        order_by,
        offset,
        limit,
    }))
}

fn parse_union_bgp(input: &str) -> Result<Option<GraphPattern>, RuntimeError> {
    let open = input
        .find('{')
        .ok_or_else(|| RuntimeError::MalformedSparql("缺少 `{`".into()))?;
    let close = matching_brace(input, open)?;
    if !input[close + 1..].trim().is_empty() {
        return Ok(None);
    }
    let content = &input[open + 1..close];
    let Some(union_at) = top_level_union(content) else {
        return Ok(None);
    };
    let left = content[..union_at].trim();
    let right = content[union_at + "UNION".len()..].trim();
    if !left.starts_with('{') || !right.starts_with('{') {
        return Ok(None);
    }
    // VALUES-only UNION 仍由既有 Values 提取器处理；此分支只构造真正的 BGP UNION。
    if keyword_position(left, "VALUES").is_some() || keyword_position(right, "VALUES").is_some() {
        return Ok(None);
    }
    let left_patterns = group(left)?;
    let right_patterns = group(right)?;
    Ok(Some(GraphPattern::Union(
        Box::new(GraphPattern::Bgp(left_patterns)),
        Box::new(GraphPattern::Bgp(right_patterns)),
    )))
}

fn top_level_union(input: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut quoted = false;
    for (index, character) in input.char_indices() {
        match character {
            '"' => quoted = !quoted,
            '{' if !quoted => depth += 1,
            '}' if !quoted => depth = depth.checked_sub(1)?,
            _ if !quoted
                && depth == 0
                && input[index..].to_ascii_uppercase().starts_with("UNION") =>
            {
                let before = input[..index].as_bytes().last().copied();
                let after = input.as_bytes().get(index + "UNION".len()).copied();
                if before.is_none_or(|byte| byte.is_ascii_whitespace())
                    && after.is_none_or(|byte| byte.is_ascii_whitespace())
                {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn extract_optionals(input: &str) -> Result<(String, Vec<Vec<TriplePattern>>), RuntimeError> {
    let mut body = input.to_owned();
    let mut optionals = Vec::new();
    loop {
        let upper = body.to_ascii_uppercase();
        let Some(start) = upper.find("OPTIONAL") else {
            break;
        };
        let after = start + "OPTIONAL".len();
        if body
            .as_bytes()
            .get(after)
            .is_some_and(|byte| !byte.is_ascii_whitespace() && *byte != b'{')
        {
            return Err(RuntimeError::MalformedSparql(
                "OPTIONAL 后缺少图模式".into(),
            ));
        }
        let open = body[after..]
            .find('{')
            .map(|offset| after + offset)
            .ok_or_else(|| RuntimeError::MalformedSparql("OPTIONAL 缺少 `{`".into()))?;
        let close = matching_brace(&body, open)?;
        optionals.push(group(&body[open..=close])?);
        body.replace_range(start..close + 1, "");
    }
    Ok((body, optionals))
}

fn extract_values(input: &str) -> Result<(String, Vec<BindingValue>), RuntimeError> {
    let mut body = input.to_owned();
    let mut values = Vec::new();
    loop {
        let Some(start) = keyword_position(&body, "VALUES") else {
            break;
        };
        let after = start + "VALUES".len();
        if body
            .as_bytes()
            .get(after)
            .is_some_and(|byte| !byte.is_ascii_whitespace())
        {
            return Err(RuntimeError::MalformedSparql("VALUES 后缺少变量".into()));
        }
        let mut index = after;
        while body
            .as_bytes()
            .get(index)
            .is_some_and(u8::is_ascii_whitespace)
        {
            index += 1;
        }
        let variable_end = body[index..]
            .find(char::is_whitespace)
            .map(|offset| index + offset)
            .ok_or_else(|| RuntimeError::MalformedSparql("VALUES 缺少值列表".into()))?;
        let variable = variable(&body[index..variable_end])?;
        let open = body[variable_end..]
            .find('{')
            .map(|offset| variable_end + offset)
            .ok_or_else(|| RuntimeError::MalformedSparql("VALUES 缺少 `{`".into()))?;
        let close = matching_brace(&body, open)?;
        for term in tokens(&body[open + 1..close])? {
            values.push(BindingValue {
                variable: variable.clone(),
                value: value_term(term)?,
            });
        }
        body.replace_range(start..close + 1, "");
    }
    // 当前阶段 UNION 只承接已提取的 VALUES 分支。剩余的 UNION/空分支标记可安全
    // 移除；包含三元组的 UNION 仍会在 group() 中以不支持的图模式失败。
    while let Some(start) = keyword_position(&body, "UNION") {
        body.replace_range(start..start + "UNION".len(), "");
    }
    Ok((body, values))
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
    Ok(Some(Aggregate {
        variable: variable.into(),
        kind,
        expression: parse_expression(expression)?,
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

fn extract_binds(input: &str) -> Result<(String, Vec<Bind>), RuntimeError> {
    let mut body = input.to_owned();
    let mut binds = Vec::new();
    loop {
        let upper = body.to_ascii_uppercase();
        let Some(start) = upper.find("BIND") else {
            break;
        };
        if body
            .as_bytes()
            .get(start + 4)
            .is_some_and(|byte| !byte.is_ascii_whitespace() && *byte != b'(')
        {
            return Err(RuntimeError::MalformedSparql("BIND 后缺少表达式".into()));
        }
        let open = body[start + 4..]
            .find('(')
            .map(|offset| start + 4 + offset)
            .ok_or_else(|| RuntimeError::MalformedSparql("BIND 缺少 `(`".into()))?;
        let close = matching_paren(&body, open)?;
        let content = body[open + 1..close].trim();
        let as_at = content
            .to_ascii_uppercase()
            .rfind(" AS ")
            .ok_or_else(|| RuntimeError::MalformedSparql("BIND 缺少 AS ?variable".into()))?;
        let expression = parse_expression(content[..as_at].trim())?;
        let variable = variable(content[as_at + 4..].trim())?;
        binds.push(Bind {
            variable,
            expression,
        });
        let mut end = close + 1;
        while body
            .as_bytes()
            .get(end)
            .is_some_and(u8::is_ascii_whitespace)
        {
            end += 1;
        }
        if body.as_bytes().get(end) == Some(&b'.') {
            end += 1;
        }
        body.replace_range(start..end, "");
    }
    Ok((body, binds))
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
    if let Some(inner) = input.strip_prefix('!').filter(|_| !input.starts_with("!=")) {
        return Ok(Expression::Not(Box::new(parse_expression(inner.trim())?)));
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
    if let Some((left, operator, right)) = split_logical_expression(input) {
        return Ok(Expression::Logical {
            operator,
            left: Box::new(parse_expression(left)?),
            right: Box::new(parse_expression(right)?),
        });
    }
    if let Some((left, operator, right)) = split_comparison_expression(input) {
        return Ok(Expression::Comparison {
            operator,
            left: Box::new(parse_expression(left)?),
            right: Box::new(parse_expression(right)?),
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
                return Ok(Expression::Aggregate {
                    kind,
                    expression: Box::new(parse_expression(argument)?),
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

fn set_pattern_base_iri(pattern: &mut GraphPattern, base_iri: Option<&str>) {
    match pattern {
        GraphPattern::Empty | GraphPattern::Bgp(_) | GraphPattern::Values(_) => {}
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
        GraphPattern::Filter(pattern, filters) => {
            set_pattern_base_iri(pattern, base_iri);
            for filter in filters {
                if let Filter::Expression(expression) = filter {
                    set_expression_base_iri(expression, base_iri);
                }
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
        Expression::Variable(_)
        | Expression::String(_)
        | Expression::TypedLiteral { .. }
        | Expression::Iri(_)
        | Expression::Number(_) => {}
    }
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
        let mut candidate = None;
        for (index, character) in input.char_indices() {
            match character {
                '\'' | '"' => quoted = !quoted,
                '(' if !quoted => depth += 1,
                ')' if !quoted => depth = depth.checked_sub(1)?,
                _ if !quoted && depth == 0 && input[index..].starts_with(needle) => {
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
    Some(input[1..end].replace(&format!("\\{quote}"), &quote.to_string()))
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
) -> Result<(Vec<String>, Vec<OrderByTerm>, Option<usize>, Option<usize>), RuntimeError> {
    let mut tail = tail.trim();
    let mut group_by = Vec::new();
    if tail.to_ascii_uppercase().starts_with("GROUP BY") {
        let rest = tail[8..].trim_start();
        let end = [
            keyword_position(rest, "ORDER BY"),
            keyword_position(rest, "OFFSET"),
            keyword_position(rest, "LIMIT"),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(rest.len());
        for token in rest[..end].split_whitespace() {
            group_by.push(variable(token)?);
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
    Ok((group_by, order_by, offset, limit))
}

fn parse_order_by(tail: &str) -> Result<Vec<OrderByTerm>, RuntimeError> {
    if tail.is_empty() {
        return Ok(Vec::new());
    }
    let terms = tail.split_whitespace().collect::<Vec<_>>();
    if terms.len() < 3
        || !terms[0].eq_ignore_ascii_case("ORDER")
        || !terms[1].eq_ignore_ascii_case("BY")
    {
        return Err(RuntimeError::UnsupportedSparql(
            "目前仅支持 ORDER BY ?variable、ASC(?variable) 与 DESC(?variable)".into(),
        ));
    }
    terms[2..]
        .iter()
        .map(|term| {
            let upper = term.to_ascii_uppercase();
            let (value, descending) = if upper.starts_with("ASC(") && term.ends_with(')') {
                (&term[4..term.len() - 1], false)
            } else if upper.starts_with("DESC(") && term.ends_with(')') {
                (&term[5..term.len() - 1], true)
            } else {
                (*term, false)
            };
            Ok(OrderByTerm {
                variable: variable(value)?,
                descending,
            })
        })
        .collect()
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
    let variable = variable(left.trim())?;
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
        if terms.len() != 3
            || !terms[1].ends_with(':')
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
                    && !matches!(
                        bytes[index],
                        b'{' | b'}' | b'.' | b';' | b'(' | b')' | b',' | b'='
                    )
                {
                    index += 1;
                }
                let token = &remaining[start..index];
                if token == "a" {
                    output.push_str(RDF_TYPE);
                } else if bytes.get(index) == Some(&b'(') {
                    // `ofn:daysBetween(...)` 之类的函数名不是 RDF term，不能展开成 IRI。
                    output.push_str(token);
                } else if let Some((prefix, local)) = token.split_once(':') {
                    let key = format!("{prefix}:");
                    let iri = prefixes.get(&key).ok_or_else(|| {
                        RuntimeError::MalformedSparql(format!("未声明的 PREFIX：{key}"))
                    })?;
                    output.push('<');
                    output.push_str(iri);
                    output.push_str(local);
                    output.push('>');
                } else {
                    output.push_str(token);
                }
            }
        }
    }
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
        if !graph.starts_with('<') || !graph.ends_with('>') {
            return Err(RuntimeError::UnsupportedSparql(
                "目前仅支持 IRI 具名图".into(),
            ));
        }
        let inner = terms[3..terms.len() - 1].to_vec();
        (inner, Some(graph.trim_matches(['<', '>']).to_owned()))
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
        let current_subject = subject.unwrap_or_else(|| terms[index]);
        if subject.is_none() {
            index += 1;
        }
        if index + 1 >= terms.len() || terms[index] == ";" || terms[index + 1] == ";" {
            return Err(RuntimeError::MalformedSparql(
                "图模式必须由完整三元组组成".into(),
            ));
        }
        if !valid_object_token(terms[index + 1]) {
            return Err(RuntimeError::MalformedSparql(
                "图模式 object 必须是变量、IRI 或合法 RDF literal".into(),
            ));
        }
        triples.push([current_subject, terms[index], terms[index + 1]]);
        index += 2;
        if terms.get(index) == Some(&";") {
            subject = Some(current_subject);
            index += 1;
        } else {
            subject = None;
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
            // `.` 既是三元组分隔符，也是裸十进制数的组成部分。只有不被两个数字
            // 包围时才作为分隔符，避免 `12.6` 被错误切成两个图模式 token。
            b'.' if start.is_some()
                && bytes
                    .get(index.saturating_sub(1))
                    .is_some_and(u8::is_ascii_digit)
                && bytes.get(index + 1).is_some_and(u8::is_ascii_digit) =>
            {
                index += 1;
            }
            b'{' | b'}' | b'.' | b';' => {
                if let Some(start) = start.take() {
                    result.push(&input[start..index]);
                }
                result.push(&input[index..index + 1]);
                index += 1;
            }
            b'<' => {
                if start.is_some() {
                    return Err(RuntimeError::MalformedSparql("IRI 前缺少分隔符".into()));
                }
                let end = input[index + 1..]
                    .find('>')
                    .map(|offset| index + offset + 1)
                    .ok_or_else(|| RuntimeError::MalformedSparql("IRI 缺少 `>`".into()))?;
                result.push(&input[index..=end]);
                index = end + 1;
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
            _ => {
                start.get_or_insert(index);
                index += 1;
            }
        }
    }
    if let Some(start) = start {
        result.push(&input[start..]);
    }
    Ok(result.into_iter().filter(|term| *term != ".").collect())
}

/// 将对象位置的单层空白节点属性列表展开为两个普通三元组。
///
/// 例如 `?p :teaches [ :duration ?d ]` 等价于
/// `?p :teaches ?__rtop_blank_0 . ?__rtop_blank_0 :duration ?d`。展开在图模式
/// 解析之前完成，因此产生的变量自然参与 JOIN、FILTER 与子查询投影。
fn expand_blank_node_property_lists(input: &str) -> String {
    let direct_pattern = Regex::new(
        r"(?s)(\?[A-Za-z_][A-Za-z0-9_]*|<[^>]+>)\s+([^\s]+)\s+\[\s*([^\s]+)\s+([^\s\]]+)\s*\]",
    )
    .expect("空白节点属性列表展开正则固定有效");
    let semicolon_pattern = Regex::new(r"(?s);\s*([^\s]+)\s+\[\s*([^\s]+)\s+([^\s\]]+)\s*\]")
        .expect("分号后的空白节点属性列表展开正则固定有效");
    let mut index = 0usize;
    let expanded = direct_pattern
        .replace_all(input, |captures: &regex::Captures<'_>| {
            let blank = format!("?__rtop_blank_{index}");
            index += 1;
            format!(
                "{} {} {blank} . {blank} {} {}",
                &captures[1], &captures[2], &captures[3], &captures[4]
            )
        })
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
    let mut output = String::new();
    let mut remaining = input;
    while let Some((before, after)) = remaining.split_once("[]") {
        output.push_str(before);
        output.push_str(&format!("?__rtop_anon_{index}"));
        index += 1;
        remaining = after;
    }
    output.push_str(remaining);
    output
}

#[cfg(test)]
mod tests {
    use super::{
        expand_blank_node_property_lists, parse, tokens, ComparisonOperator, Expression, Filter,
        GraphPattern, Query,
    };

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
    fn accepts_crlf_prefix_declarations_from_manifest_queries() {
        let query = parse(
            "PREFIX : <https://example.test/>\r\nSELECT ?x WHERE { ?x a :Thing } ORDER BY ?x",
        )
        .expect("CRLF 的 PREFIX 查询应与 LF 查询同样可解析");
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
    fn keeps_a_bare_decimal_as_one_graph_pattern_token() {
        assert_eq!(
            tokens("?x <https://example.test/p> 12.6 .").unwrap(),
            ["?x", "<https://example.test/p>", "12.6"]
        );
    }
}
