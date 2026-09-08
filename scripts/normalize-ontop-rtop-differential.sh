#!/usr/bin/env sh
set -eu

# 仅规范化两个独立进程已经产生的外部结果。该脚本不导入 rtop 代码，且其
# 内容哈希是 passed 差分证据的一部分；case 路由或运行编排的变化不会改变它。
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
comparison=${1:?需要 comparison}
case_name=${2:?需要 case_name}
artifacts=${3:?需要 artifacts 目录}
student_kind=${4:-}

# 对 PostgreSQL FLOAT 的 xsd:double lexical 采用 RDF numeric value 的 canonical
# scientific form。Ontop CLI 可输出 "30"，而固定基线 fixture 和 rtop 输出
# "3.0E1"；两者是同一 double value，不应构成差分失败。
canonicalize_xsd_double() {
  awk '
    function canonical(value, scientific, parts, mantissa, exponent) {
      if (value == "NaN" || value == "INF" || value == "-INF") return value
      scientific = sprintf("%.15e", value + 0)
      split(scientific, parts, "e")
      mantissa = parts[1]
      sub(/0+$/, "", mantissa)
      if (mantissa !~ /\./) mantissa = mantissa ".0"
      if (mantissa ~ /\.$/) mantissa = mantissa "0"
      exponent = parts[2] + 0
      return mantissa "E" exponent
    }
    {
      datatype = "^^<http://www.w3.org/2001/XMLSchema#double>"
      if (match($0, /"[^"]*"\^\^<http:\/\/www\.w3\.org\/2001\/XMLSchema#double>/)) {
        term = substr($0, RSTART, RLENGTH)
        lexical = term
        sub(/^"/, "", lexical)
        sub(/"\^\^<.*$/, "", lexical)
        replacement = "\"" canonical(lexical) datatype
        $0 = substr($0, 1, RSTART - 1) replacement substr($0, RSTART + RLENGTH)
      }
      print
    }
  '
}

# D005b/D012a/D012b 都是由 literal 邻接关系区分的 blank nodes。输入已经排序，故
# 按邻接签名重标会保留这些 fixture 的图同构关系；不将此简化器用于一般图。
canonicalize_d005_blank_nodes() {
  awk '
    {
      lines[NR] = $0
      if ($1 ~ /^_:/ && !($1 in labels)) {
        labels[$1] = 1
        label_count++
      }
    }
    END {
      for (label in labels) {
        signature[label] = ""
        for (line_number = 1; line_number <= NR; line_number++) {
          line = lines[line_number]
          if (index(line, label " ") == 1) {
            sub(/^_:[^ ]+/, "_:self", line)
            signature[label] = signature[label] line "\n"
          }
        }
      }
      for (rank = 1; rank <= label_count; rank++) {
        selected = ""
        for (label in labels) {
          if (!(label in assigned) && (selected == "" || signature[label] < signature[selected])) selected = label
        }
        assigned[selected] = 1
        replacement[selected] = "_:b" rank
      }
      for (line_number = 1; line_number <= NR; line_number++) {
        line = lines[line_number]
        split(line, fields, " ")
        sub(/^_:[^ ]+/, replacement[fields[1]], line)
        print line
      }
    }
  '
}

# D014 的 parent blank node 同时出现在 EMP 外键 triple 的 object 位置。此固定
# fixture 只使用 blank node 作 subject/object，且每个节点都至少有 subject
# triples；按 subject 邻接排序后，也替换 object 位置以校验跨 relation identity。
canonicalize_d014_blank_nodes() {
  awk '
    {
      lines[NR] = $0
      if ($1 ~ /^_:/ && !($1 in labels)) {
        labels[$1] = 1
        label_count++
      }
    }
    END {
      for (label in labels) {
        signature[label] = ""
        for (line_number = 1; line_number <= NR; line_number++) {
          line = lines[line_number]
          if (index(line, label " ") == 1) {
            sub(/^_:[^ ]+/, "_:self", line)
            signature[label] = signature[label] line "\n"
          }
        }
      }
      for (rank = 1; rank <= label_count; rank++) {
        selected = ""
        for (label in labels) {
          if (!(label in assigned) && (selected == "" || signature[label] < signature[selected])) selected = label
        }
        assigned[selected] = 1
        replacement[selected] = "_:b" rank
      }
      for (line_number = 1; line_number <= NR; line_number++) {
        line = lines[line_number]
        split(line, fields, " ")
        if (fields[1] in replacement || fields[3] in replacement) {
          subject = (fields[1] in replacement ? replacement[fields[1]] : fields[1])
          object = (fields[3] in replacement ? replacement[fields[3]] : fields[3])
          sub(/^[^ ]+ [^ ]+ [^ ]+/, subject " " fields[2] " " object, line)
        }
        print line
      }
    }
  '
}

if [ "$comparison" = materialize ] || [ "$comparison" = bootstrap-materialize ] || [ "$comparison" = pretty-materialize ] || [ "$comparison" = to-obda-materialize ] || [ "$comparison" = to-r2rml-materialize ] || [ "$comparison" = v1-to-v3-materialize ] || [ "$comparison" = http-predefined ]; then
  # N-Quads 保留 RDF 项的 datatype；排序仅消除 materializer 的枚举顺序。
  if [ "$case_name" = d002b ] || [ "$case_name" = d002d ] || [ "$case_name" = dmd001 ] || [ "$case_name" = dmd002 ] || [ "$case_name" = dmd003 ] || [ "$case_name" = dmd004 ] || [ "$case_name" = dmd017 ] || [ "$case_name" = clitor2rmltypedbnode ]; then
    # blank-node 标签由各实现自行分配。该 fixture 必须且只会公开一个 blank
    # node，故先证明其基数再固定重标；多节点图不得使用这一简化规范化。
    # 以完整的第一 RDF 项作为标签，不假定实现使用某一特定 bnode 字符集。
    # rtop 的 Direct Mapping 行标识会包含括号和逗号，不能用简化正则截断。
    ontop_bnodes=$(awk '$1 ~ /^_:/ { print $1 }' "$artifacts/ontop.raw" | LC_ALL=C sort -u)
    rtop_bnodes=$(awk '$1 ~ /^_:/ { print $1 }' "$artifacts/rtop.raw" | LC_ALL=C sort -u)
    [ "$(printf '%s\n' "$ontop_bnodes" | sed '/^$/d' | wc -l | tr -d ' ')" = 1 ]
    [ "$(printf '%s\n' "$rtop_bnodes" | sed '/^$/d' | wc -l | tr -d ' ')" = 1 ]
    awk -v label="$ontop_bnodes" '$1 == label { sub(/^[^ ]+/, "_:b0") } { print }' "$artifacts/ontop.raw" | LC_ALL=C sort > "$artifacts/ontop.normalized.nq"
    awk -v label="$rtop_bnodes" '$1 == label { sub(/^[^ ]+/, "_:b0") } { print }' "$artifacts/rtop.raw" | LC_ALL=C sort > "$artifacts/rtop.normalized.nq"
  elif [ "$case_name" = d005a ] || [ "$case_name" = d016b ] || [ "$case_name" = dmd016 ]; then
    canonicalize_xsd_double < "$artifacts/ontop.raw" | LC_ALL=C sort > "$artifacts/ontop.normalized.nq"
    canonicalize_xsd_double < "$artifacts/rtop.raw" | LC_ALL=C sort > "$artifacts/rtop.normalized.nq"
  elif [ "$case_name" = d005b ] || [ "$case_name" = d012a ] || [ "$case_name" = d012b ] || [ "$case_name" = d012e ] || [ "$case_name" = dmd005 ] || [ "$case_name" = dmd012 ] || [ "$case_name" = dmd018 ]; then
    canonicalize_xsd_double < "$artifacts/ontop.raw" | LC_ALL=C sort | canonicalize_d005_blank_nodes | LC_ALL=C sort > "$artifacts/ontop.normalized.nq"
    canonicalize_xsd_double < "$artifacts/rtop.raw" | LC_ALL=C sort | canonicalize_d005_blank_nodes | LC_ALL=C sort > "$artifacts/rtop.normalized.nq"
  elif [ "$case_name" = dmd014 ] || [ "$case_name" = dmd022 ] || [ "$case_name" = dmd025 ]; then
    canonicalize_d014_blank_nodes < "$artifacts/ontop.raw" | LC_ALL=C sort > "$artifacts/ontop.normalized.nq"
    canonicalize_d014_blank_nodes < "$artifacts/rtop.raw" | LC_ALL=C sort > "$artifacts/rtop.normalized.nq"
  else
    # 空 N-Quads graph 可表现为空文件或单个行终止符；二者没有 RDF term 差异。
    sed '/^[[:space:]]*$/d' "$artifacts/ontop.raw" | LC_ALL=C sort > "$artifacts/ontop.normalized.nq"
    sed '/^[[:space:]]*$/d' "$artifacts/rtop.raw" | LC_ALL=C sort > "$artifacts/rtop.normalized.nq"
  fi
  jq -Rsc 'split("\n") | map(select(length > 0)) | {format:"nquads", ordered:false, triples:.}' \
    < "$artifacts/ontop.normalized.nq" > "$artifacts/normalized.json"
  cmp -s "$artifacts/ontop.normalized.nq" "$artifacts/rtop.normalized.nq"
elif [ "$comparison" = http-result-formats ]; then
  formats_match=true
  for side in ontop rtop; do
    if ! jq -e '
      .json.status == "200"
      and (.json.body | fromjson | .head.vars == ["person", "name"])
      and (.json.body | fromjson | .results.bindings | length == 1)
      and (.json.body | fromjson | .results.bindings[0].person.value == "https://example.test/person/1")
      and (.json.body | fromjson | .results.bindings[0].name.value == "Ada")
      and .xml.status == "200"
      and (.xml.body | test("<sparql"; "i") and test("https://example.test/person/1") and test(">Ada<"))
      and .csv.status == "200"
      and (.csv.body | test("person,name") and test("https://example.test/person/1") and test("Ada"))
      and .tsv.status == "200"
      and (.tsv.body | test("\\?person\\t\\?name") and test("<https://example.test/person/1>") and test("\\\"Ada\\\""))
      and .ntriples.status == "200"
      and (.ntriples.body | test("<https://example.test/person/1> <https://example.test/name>") and test("Ada"))
      and .unsupported.status == "406"
    ' "$artifacts/$side.raw" >/dev/null; then
      formats_match=false
    fi
  done
  jq -n '{format:"http-result-formats",select_formats:["application/sparql-results+json","application/sparql-results+xml","text/csv","text/tab-separated-values"],construct_format:"application/n-triples",unsupported_accept_status:406,person:"https://example.test/person/1",name:"Ada"}' > "$artifacts/normalized.json"
  [ "$formats_match" = true ]
elif [ "$comparison" = http-concurrency-isolation ]; then
  isolation_match=true
  for side in ontop rtop; do
    if ! jq -e '.delayed.status == "200" and (.delayed.body | fromjson | .boolean == false) and .ask.status == "200" and (.ask.body | fromjson | .boolean == true) and .disconnect.curl_exit == 28 and .disconnect.active_delays == "0" and .reuse.status == "200" and (.reuse.body | fromjson | .boolean == true)' "$artifacts/$side.raw" >/dev/null; then
      isolation_match=false
    fi
  done
  jq -n '{format:"http-concurrency-isolation-and-disconnect",delayed:"pg_sleep(5) ASK false",concurrent_ask:true,disconnect_curl_exit:28,active_delays:0,reuse_ask:true}' > "$artifacts/normalized.json"
  [ "$isolation_match" = true ]
elif [ "$comparison" = http-json ] && [ "$case_name" = httpnativetermsnull ]; then
  terms_null_match=true
  for side in ontop rtop; do
    if ! jq -e '
      .head.vars == ["person", "label"]
      and (.results.bindings | length == 2)
      and ([.results.bindings[] | select(.person.value == "https://example.test/person/1") | .label] == [{"type":"literal","value":"Ada","xml:lang":"en"}])
      and ([.results.bindings[] | select(.person.value == "https://example.test/nullable/1") | .label] == [{"type":"literal","value":"visible"}])
      and ([.results.bindings[] | select(.person.value == "https://example.test/person/2")] | length == 0)
    ' "$artifacts/$side.raw" >/dev/null; then
      terms_null_match=false
    fi
  done
  jq -n '{format:"http-native-obda-terms-null",language_literal:{value:"Ada",language:"en"},string_literal:"visible",null_person_omitted:true}' > "$artifacts/normalized.json"
  [ "$terms_null_match" = true ]
elif [ "$comparison" = http-json ]; then
  # SPARQL Results JSON 是双方 HTTP 进程的公开协议；键、变量和无序 binding
  # 均由 jq 排序，保留 IRI、literal、datatype 与 graph binding。不能直接以
  # tojson 作为排序键：它会保留对象的插入顺序，导致字段顺序不同的同一 binding
  # 排出不同次序。先递归 canonical，再生成排序键。
  canonical_results='
    def canonical:
      if type == "object" then
        to_entries | sort_by(.key) | map({key:.key, value:(.value | canonical)}) | from_entries
      elif type == "array" then map(canonical)
      else . end;
    .head.vars |= sort
    | .results.bindings |= (map(canonical) | sort_by(tojson))
    | canonical
  '
  jq -S "$canonical_results" "$artifacts/ontop.raw" > "$artifacts/ontop.normalized.json"
  jq -S "$canonical_results" "$artifacts/rtop.raw" > "$artifacts/rtop.normalized.json"
  cp "$artifacts/ontop.normalized.json" "$artifacts/normalized.json"
  cmp -s "$artifacts/ontop.normalized.json" "$artifacts/rtop.normalized.json"
elif [ "$comparison" = http-ask ]; then
  # ASK 的 SPARQL Results JSON 只有 boolean；显式保留协议形状，不把它伪装成
  # 空 binding 的 SELECT 结果。
  jq -S 'select(.boolean | type == "boolean") | {head:(.head // {}),boolean}' "$artifacts/ontop.raw" > "$artifacts/ontop.normalized.json"
  jq -S 'select(.boolean | type == "boolean") | {head:(.head // {}),boolean}' "$artifacts/rtop.raw" > "$artifacts/rtop.normalized.json"
  cp "$artifacts/ontop.normalized.json" "$artifacts/normalized.json"
  cmp -s "$artifacts/ontop.normalized.json" "$artifacts/rtop.normalized.json"
elif [ "$comparison" = http-protocol ]; then
  jq -S 'all(.[]; .status == "200" and .body == {head:{},boolean:true})' "$artifacts/ontop.raw" >/dev/null
  jq -S 'all(.[]; .status == "200" and .body == {head:{},boolean:true})' "$artifacts/rtop.raw" >/dev/null
  jq -S . "$artifacts/ontop.raw" > "$artifacts/ontop.normalized.json"
  jq -S . "$artifacts/rtop.raw" > "$artifacts/rtop.normalized.json"
  cp "$artifacts/ontop.normalized.json" "$artifacts/normalized.json"
  cmp -s "$artifacts/ontop.normalized.json" "$artifacts/rtop.normalized.json"
elif [ "$comparison" = http-reformulate ]; then
  for side in ontop rtop; do
    jq -e '
      .default.status == "200"
      and (.default.body | test("SELECT[[:space:]]"))
      and (.default.query_id | test("^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"; "i"))
      and .native.status == "200"
      and (.native.body | test("^SELECT[[:space:]]"))
      and (.native.query_id | test("^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"; "i"))
    ' "$artifacts/$side.raw" >/dev/null
  done
  jq -n '{format:"development-reformulate",paths:["default","forNativeConsumption=true"],status:"200",sql_diagnostic:true,query_id:"uuid-v4"}' > "$artifacts/normalized.json"
elif [ "$comparison" = http-error ]; then
  sed '/^[[:space:]]*$/d' "$artifacts/ontop.raw" > "$artifacts/ontop.normalized.txt"
  sed '/^[[:space:]]*$/d' "$artifacts/rtop.raw" > "$artifacts/rtop.normalized.txt"
  [ "$(cat "$artifacts/ontop.normalized.txt")" = 500 ]
  [ "$(cat "$artifacts/rtop.normalized.txt")" = 500 ]
  grep -qi 'EXISTS subquery.*unbound' "$artifacts/ontop.body"
  grep -qi 'variables.*EXISTS.*unbound' "$artifacts/rtop.body"
  jq -n --arg status 500 --arg category 'correlated-exists-unbound' '{format:"http-error",status:$status,category:$category}' > "$artifacts/normalized.json"
elif [ "$comparison" = http-parse-error ]; then
  sed '/^[[:space:]]*$/d' "$artifacts/ontop.raw" > "$artifacts/ontop.normalized.txt"
  sed '/^[[:space:]]*$/d' "$artifacts/rtop.raw" > "$artifacts/rtop.normalized.txt"
  [ "$(cat "$artifacts/ontop.normalized.txt")" = 400 ]
  [ "$(cat "$artifacts/rtop.normalized.txt")" = 400 ]
  grep -Eqi 'Lexical error|Encountered .* at line' "$artifacts/ontop.body"
  grep -qi 'malformed-sparql' "$artifacts/rtop.body"
  jq -n --arg status 400 --arg category malformed-sparql '{format:"http-error",status:$status,category:$category}' > "$artifacts/normalized.json"
elif [ "$comparison" = http-invalid-datetime-lexical ]; then
  sed '/^[[:space:]]*$/d' "$artifacts/ontop.raw" > "$artifacts/ontop.normalized.txt"
  sed '/^[[:space:]]*$/d' "$artifacts/rtop.raw" > "$artifacts/rtop.normalized.txt"
  [ "$(cat "$artifacts/ontop.normalized.txt")" = 500 ]
  [ "$(cat "$artifacts/rtop.normalized.txt")" = 500 ]
  grep -qi 'Invalid lexical forms.*xsd:dateTime' "$artifacts/ontop.body"
  grep -qi 'type-error:.*xsd:dateTime lexical form' "$artifacts/rtop.body"
  jq -n --arg status 500 --arg category invalid-datetime-lexical '{format:"http-error",status:$status,category:$category}' > "$artifacts/normalized.json"
elif [ "$comparison" = http-invalid-boolean-lexical ]; then
  sed '/^[[:space:]]*$/d' "$artifacts/ontop.raw" > "$artifacts/ontop.normalized.txt"
  sed '/^[[:space:]]*$/d' "$artifacts/rtop.raw" > "$artifacts/rtop.normalized.txt"
  [ "$(cat "$artifacts/ontop.normalized.txt")" = 500 ]
  [ "$(cat "$artifacts/rtop.normalized.txt")" = 500 ]
  grep -qi 'Invalid lexical forms.*xsd:boolean' "$artifacts/ontop.body"
  grep -qi 'type-error:.*xsd:boolean lexical form' "$artifacts/rtop.body"
  jq -n --arg status 500 --arg category invalid-boolean-lexical '{format:"http-error",status:$status,category:$category}' > "$artifacts/normalized.json"
elif [ "$comparison" = http-native-source-sql-error ]; then
  sed '/^[[:space:]]*$/d' "$artifacts/ontop.raw" > "$artifacts/ontop.normalized.txt"
  sed '/^[[:space:]]*$/d' "$artifacts/rtop.raw" > "$artifacts/rtop.normalized.txt"
  [ "$(cat "$artifacts/ontop.normalized.txt")" = 500 ]
  [ "$(cat "$artifacts/rtop.normalized.txt")" = 500 ]
  grep -qi 'syntax error' "$artifacts/ontop.body"
  grep -qi 'datasource-failure: db error' "$artifacts/rtop.body"
  jq -n --arg status 500 --arg category native-source-sql-runtime-error '{format:"http-error",status:$status,category:$category}' > "$artifacts/normalized.json"
elif [ "$comparison" = http-service-error ]; then
  sed '/^[[:space:]]*$/d' "$artifacts/ontop.raw" > "$artifacts/ontop.normalized.txt"
  sed '/^[[:space:]]*$/d' "$artifacts/rtop.raw" > "$artifacts/rtop.normalized.txt"
  [ "$(cat "$artifacts/ontop.normalized.txt")" = 500 ]
  [ "$(cat "$artifacts/rtop.normalized.txt")" = 500 ]
  grep -qi 'SERVICE' "$artifacts/ontop.body"
  grep -qi 'SERVICE.*未支持' "$artifacts/rtop.body"
  jq -n --arg status 500 --arg category service-unsupported '{format:"http-error",status:$status,category:$category}' > "$artifacts/normalized.json"
elif [ "$comparison" = compile ] || [ "$comparison" = validate ]; then
  # compile 的固定基线是零输出 no-op；validate 则固定报告 Validation completed。
  # 两者均比较去除行终止符后的 CLI stdout，原始结果仍完整保留作取证。
  tr -d '\r\n' < "$artifacts/ontop.raw" > "$artifacts/ontop.normalized.txt"
  tr -d '\r\n' < "$artifacts/rtop.raw" > "$artifacts/rtop.normalized.txt"
  jq -n --arg ontop "$(cat "$artifacts/ontop.normalized.txt")" --arg rtop "$(cat "$artifacts/rtop.normalized.txt")" \
    '{format:"cli-stdout",ontop:$ontop,rtop:$rtop}' > "$artifacts/normalized.json"
  cmp -s "$artifacts/ontop.normalized.txt" "$artifacts/rtop.normalized.txt"
elif [ "$comparison" = simple-csv ]; then
  # Ontop CLI query 的 CSV 结果与 rtop CLI 的 RDF-term 行格式不同。该固定的
  # 单变量 IRI fixture 只移除 CSV header，并显式重建同一 CLI term 表示。
  # Ontop CLI 可能在 CSV header 前写入启动日志，因此只从固定 header 之后
  # 读取非空行；日志不属于 query 的 CSV 结果。
  awk '$0 == "resource" { rows = 1; next } rows && NF { print "?resource=<" $0 ">" }' "$artifacts/ontop.raw" | LC_ALL=C sort > "$artifacts/ontop.normalized.txt"
  [ -s "$artifacts/ontop.normalized.txt" ]
  sed '/^[[:space:]]*$/d' "$artifacts/rtop.raw" | LC_ALL=C sort > "$artifacts/rtop.normalized.txt"
  jq -Rsc 'split("\\n") | map(select(length > 0)) | {format:"select-bindings",ordered:false,rows:.}' \
    < "$artifacts/ontop.normalized.txt" > "$artifacts/normalized.json"
  cmp -s "$artifacts/ontop.normalized.txt" "$artifacts/rtop.normalized.txt"
elif [ "$comparison" = metadata ]; then
  # 固定 PostgreSQL 三表 fixture 只比较可重新加载/查询所需的 catalog 核心：
  # relation、列、主键与可空性。JDBC driver/extraction timestamp、空 foreignKeys
  # 以及 PostgreSQL 同义 type lexical 不是跨实现的 schema 语义差异。
  canonical_metadata='{
    relations: [
      .relations[]
      | {
          name,
          columns: [ .columns[] | {name, isNullable, datatype: (.datatype | ascii_downcase | if . == "int4" or . == "integer" then "integer" elif startswith("text") then "text" else . end) } ],
          uniqueConstraints: [ .uniqueConstraints[] | {name, determinants, isPrimaryKey} ]
        }
    ] | sort_by(.name)
  }'
  jq -S "$canonical_metadata" "$artifacts/ontop.raw" > "$artifacts/ontop.normalized.json"
  jq -S "$canonical_metadata" "$artifacts/rtop.raw" > "$artifacts/rtop.normalized.json"
  cp "$artifacts/ontop.normalized.json" "$artifacts/normalized.json"
  cmp -s "$artifacts/ontop.normalized.json" "$artifacts/rtop.normalized.json"
elif [ "$comparison" = http-status ]; then
  sed '/^[[:space:]]*$/d' "$artifacts/ontop.raw" > "$artifacts/ontop.normalized.txt"
  sed '/^[[:space:]]*$/d' "$artifacts/rtop.raw" > "$artifacts/rtop.normalized.txt"
  [ "$(cat "$artifacts/ontop.normalized.txt")" = 404 ]
  [ "$(cat "$artifacts/rtop.normalized.txt")" = 404 ]
  jq -n --arg ontop "$(cat "$artifacts/ontop.normalized.txt")" --arg rtop "$(cat "$artifacts/rtop.normalized.txt")" \
    '{format:"http-status",ontop:$ontop,rtop:$rtop}' > "$artifacts/normalized.json"
  cmp -s "$artifacts/ontop.normalized.txt" "$artifacts/rtop.normalized.txt"
elif [ "$comparison" = http-predefined-invalid ]; then
  sed '/^[[:space:]]*$/d' "$artifacts/ontop.raw" > "$artifacts/ontop.normalized.txt"
  sed '/^[[:space:]]*$/d' "$artifacts/rtop.raw" > "$artifacts/rtop.normalized.txt"
  [ "$(cat "$artifacts/ontop.normalized.txt")" = 500 ]
  [ "$(cat "$artifacts/rtop.normalized.txt")" = 500 ]
  jq -n --arg ontop "$(cat "$artifacts/ontop.normalized.txt")" --arg rtop "$(cat "$artifacts/rtop.normalized.txt")" \
    '{format:"http-predefined-invalid",ontop:$ontop,rtop:$rtop}' > "$artifacts/normalized.json"
  cmp -s "$artifacts/ontop.normalized.txt" "$artifacts/rtop.normalized.txt"
elif [ "$comparison" = http-status-body ]; then
  sed '/^[[:space:]]*$/d' "$artifacts/ontop.raw" > "$artifacts/ontop.normalized.txt"
  sed '/^[[:space:]]*$/d' "$artifacts/rtop.raw" > "$artifacts/rtop.normalized.txt"
  [ "$(cat "$artifacts/ontop.normalized.txt")" = 404 ]
  [ "$(cat "$artifacts/rtop.normalized.txt")" = 404 ]
  [ "$(cat "$artifacts/ontop.post.status")" = 404 ]
  [ "$(cat "$artifacts/rtop.post.status")" = 404 ]
  cmp -s "$artifacts/ontop.body" "$artifacts/rtop.body"
  [ "$(cat "$artifacts/ontop.body")" = 'No ontology found' ]
  jq -n --arg status "$(cat "$artifacts/ontop.normalized.txt")" --arg body "$(cat "$artifacts/ontop.body")" --arg post_status "$(cat "$artifacts/ontop.post.status")" \
    '{format:"http-status-body",get_status:$status,get_body:$body,post_status:$post_status}' > "$artifacts/normalized.json"
  cmp -s "$artifacts/ontop.normalized.txt" "$artifacts/rtop.normalized.txt"
elif [ "$comparison" = http-ontology-content ]; then
  sed '/^[[:space:]]*$/d' "$artifacts/ontop.raw" > "$artifacts/ontop.normalized.txt"
  sed '/^[[:space:]]*$/d' "$artifacts/rtop.raw" > "$artifacts/rtop.normalized.txt"
  [ "$(cat "$artifacts/ontop.normalized.txt")" = 200 ]
  [ "$(cat "$artifacts/rtop.normalized.txt")" = 200 ]
  [ "$(cat "$artifacts/ontop.post.status")" = 200 ]
  [ "$(cat "$artifacts/rtop.post.status")" = 200 ]
  for body in "$artifacts/ontop.body" "$artifacts/rtop.body" "$artifacts/ontop.post.body" "$artifacts/rtop.post.body"; do
    grep -F 'http://it.unibz.inf/obda/test/simple#A' "$body" >/dev/null
  done
  jq -n --arg get_status "$(cat "$artifacts/ontop.normalized.txt")" --arg post_status "$(cat "$artifacts/ontop.post.status")" \
    '{format:"ontology-document",get_status:$get_status,post_status:$post_status,required_iri:"http://it.unibz.inf/obda/test/simple#A"}' > "$artifacts/normalized.json"
  cmp -s "$artifacts/ontop.normalized.txt" "$artifacts/rtop.normalized.txt"
elif [ "$comparison" = replacement-contract ]; then
  # 这些交付/驱动/外部 fixture 原子在固定 Ontop 源码树中没有同构宿主 API。
  # 不能伪装成两端 JSON 相等；保留 Ontop 的明确非同构声明，并把 rtop 实测
  # PostgreSQL contract 原样纳入规范化证据，仍可从不可变 raw 独立复核。
  jq -e '
    .baseline == "5ec07573b18513f33dfcd59ac45fe26a81f9cdbd"
    and (.result | type == "string")
    and (.result | test("没有|no Rust-equivalent|no isomorphic|cannot falsely compare"; "i"))
  ' "$artifacts/ontop.raw" >/dev/null
  jq -e '.result == "passed" and (.postgres_image | startswith("postgres:17@sha256:"))' \
    "$artifacts/rtop.raw" >/dev/null
  jq -n -S --arg case_name "$case_name" --slurpfile rtop "$artifacts/rtop.raw" \
    '{format:"replacement-contract",case_name:$case_name,
      ontop_has_no_isomorphic_artifact:true,rtop_contract:$rtop[0]}' \
    > "$artifacts/normalized.json"
elif [ "$comparison" = replacement-facts-rejection ]; then
  # Facts 文件缺失/畸形在两端都必须在 endpoint health 前拒绝；Java/Rust 的
  # 异常类型不同，故比较稳定拒绝阶段与类别而非堆栈文本。
  grep -qi 'FactsException' "$artifacts/ontop.raw"
  grep -qi 'invalid-facts:' "$artifacts/rtop.raw"
  jq -n '{format:"facts-endpoint-startup-rejection",cases:["missing","malformed"],ontop_category:"FactsException",rtop_category:"invalid-facts",both_rejected_before_health:true}' \
    > "$artifacts/normalized.json"
elif [ "$comparison" = artifact-aggregation ]; then
  # 父 artifact 的 raw 是子 artifact 原始输出的不可变索引。逐 side 校验每个
  # case_id/hash 可在本仓库中找到，并且确实等于保存的子 raw。
  jq -e '.components | type == "array" and length > 0' "$artifacts/ontop.raw" >/dev/null
  jq -e '.components | type == "array" and length > 0' "$artifacts/rtop.raw" >/dev/null
  ontop_ids=$(jq -c '[.components[] | if type == "object" then .case_id else . end]' "$artifacts/ontop.raw")
  rtop_ids=$(jq -c '[.components[] | if type == "object" then .case_id else . end]' "$artifacts/rtop.raw")
  [ "$ontop_ids" = "$rtop_ids" ]
  for side in ontop rtop; do
    jq -r '.components[] | select(type == "object") | [.case_id, .sha256] | @tsv' "$artifacts/$side.raw" |
      while IFS="$(printf '\t')" read -r component_id expected_hash; do
        component_provenance=$(rg -l --glob provenance.json -F "\"case_id\": \"$component_id\"" "$root/docs/research/differential-artifacts" | sed -n '1p')
        [ -n "$component_provenance" ]
        component_dir=$(dirname "$component_provenance")
        [ "$(sha256sum "$component_dir/$side.raw" | awk '{print $1}')" = "$expected_hash" ]
      done
  done
  jq -n --argjson components "$ontop_ids" \
    '{format:"differential-artifact-aggregation",all_components_passed:true,components:$components}' > "$artifacts/normalized.json"
elif [ "$comparison" = suite-provenance ]; then
  # suite 父 raw 是子输出拼接，语义清单来自子 provenance。逐个复核子 raw
  # hash 与 passed 状态，再从子 provenance 重建父级 cases。
  child_provenances=
  # suite runner 在原始输出中以 ### case_name 保留执行顺序；该顺序是部分
  # 聚合结果的可观察 bag/order 证据。没有该标记的旧 suite 才采用稳定目录序。
  for child_name in $(rg -o '### [[:alnum:]_-]+' "$artifacts/ontop.raw" | sed 's/^### //'); do
    child_provenance=$(find "$artifacts" -mindepth 1 -type d -name "$child_name" -exec sh -c '[ -f "$1/provenance.json" ] && printf "%s\n" "$1/provenance.json"' sh {} \; | sed -n '1p')
    [ -n "$child_provenance" ]
    child_provenances="${child_provenances}${child_provenance}
"
  done
  if [ -z "$child_provenances" ]; then
    child_provenances=$(find "$artifacts" -mindepth 2 -name provenance.json | LC_ALL=C sort)
  fi
  [ -n "$child_provenances" ]
  for child in $child_provenances; do
    jq -e '.status == "passed"' "$child" >/dev/null
    child_dir=$(dirname "$child")
    for side in ontop rtop; do
      if [ "$side" = ontop ]; then expected_hash=$(jq -r '.sha256.ontop_raw' "$child"); else expected_hash=$(jq -r '.sha256.rtop_raw' "$child"); fi
      [ "$(sha256sum "$child_dir/$side.raw" | awk '{print $1}')" = "$expected_hash" ]
    done
  done
  cases=$(printf '%s\n' "$child_provenances" | xargs jq -s 'map({case_id,status,mapping_behavior})')
  [ "$student_kind" = all_passed ] || [ "$student_kind" = all_rejected ]
  jq -n --arg format "$case_name" --argjson cases "$cases" --arg result_key "$student_kind" \
    '{format:$format,cases:$cases} + {($result_key):true}' > "$artifacts/normalized.json"
elif [ "$comparison" = rejection ] || [ "$comparison" = native-reader-rejection ] || [ "$comparison" = native-source-relation-error ] || [ "$comparison" = conversion-rejection ] || [ "$comparison" = v1-to-v3-rejection ]; then
  # artifact 保存的是 stderr，而 shell exit code 属于 harness 的瞬时状态且未
  # 写入原始证据。仅以可复核的双方稳定错误类别证明拒绝，绝不从旧 normalized
  # 文件回填或猜测退出码。
  grep -Eqi 'Exception|FAILED TO PARSE|Invalid language tag|Error occurred during v1-to-v3' "$artifacts/ontop.raw"
  grep -Eqi 'invalid-mapping:|datasource-failure: db error' "$artifacts/rtop.raw"
  if [ "$comparison" = native-reader-rejection ]; then
    grep -qi 'Invalid target' "$artifacts/ontop.raw"
    normalized_format=native-mapping-reader-rejection
  elif [ "$comparison" = native-source-relation-error ]; then
    grep -qi 'Cannot find relation.*person' "$artifacts/ontop.raw"
    normalized_format=native-source-relation-runtime-error
  elif [ "$comparison" = conversion-rejection ]; then
    normalized_format=conversion-rejection
  elif [ "$comparison" = v1-to-v3-rejection ]; then
    normalized_format=v1-to-v3-rejection
  else
    normalized_format=mapping-rejection
  fi
  jq -n --arg format "$normalized_format" '{format:$format,both_rejected:true}' > "$artifacts/normalized.json"
elif [ "$comparison" = query ]; then
  # Ontop CLI 的 CSV 查询结果与 rtop 的 RDF-term 行格式不同。此固定的
  # student/name fixture 精确比较 literal 与 IRI；blank node 仅比较存在性，
  # 再以稳定标签 b0 表示两端实现各自分配的标识符。
  ontop_name=$(awk -F, '$0 == "student,name" { header = 1; next } header && NF == 2 { print $2; exit }' "$artifacts/ontop.raw")
  ontop_student=$(awk -F, '$0 == "student,name" { header = 1; next } header && NF == 2 { print $1; exit }' "$artifacts/ontop.raw")
  rtop_name=$(sed -n 's/.*?name="\([^"]*\)".*/\1/p' "$artifacts/rtop.raw" | head -n 1)
  if [ "$student_kind" = iri ]; then
    rtop_student=$(sed -n 's/.*?student=<\([^>]*\)>.*/\1/p' "$artifacts/rtop.raw" | head -n 1)
  else
    rtop_student=$(sed -n 's/.*?student=_:\([^ ]*\).*/\1/p' "$artifacts/rtop.raw" | head -n 1)
  fi
  [ "$ontop_name" = "$rtop_name" ]
  if [ "$student_kind" = iri ]; then
    [ "$ontop_student" = "$rtop_student" ]
    normalized_student=$ontop_student
  else
    [ -n "$ontop_student" ]
    [ -n "$rtop_student" ]
    normalized_student=b0
  fi
  jq -n --arg name "$ontop_name" --arg student "$normalized_student" --arg kind "$student_kind" \
    '{vars:["name","student"], ordered:false, rows:[{name:{kind:"literal",value:$name},student:{kind:$kind,value:$student}}]}' \
    > "$artifacts/normalized.json"
else
  printf '%s\n' "unsupported differential comparison: $comparison" >&2
  exit 64
fi
