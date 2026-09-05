#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
name=rtop-postgres-compat-$$
scratch=$(mktemp -d)
cleanup() {
  docker rm -f "$name" >/dev/null 2>&1 || true
  rm -rf "$scratch"
}
trap cleanup EXIT INT TERM
trap 'status=$?; printf "postgres-compat failure: line=%s status=%s command=%s\\n" "$LINENO" "$status" "$BASH_COMMAND" >&2' ERR
assert_output() {
  actual_normalized=$(printf '%s\n' "$1" | tr '\t' ' ' | tr -d '\r' | sed '/^[[:space:]]*$/d')
  expected_normalized=$(printf '%s\n' "$2" | tr '\t' ' ' | tr -d '\r' | sed '/^[[:space:]]*$/d')
  [ "$actual_normalized" = "$expected_normalized" ]
}
canonicalize_blank_nodes() {
  awk '{
    for (field_index = 1; field_index <= NF; field_index++) {
      if ($field_index ~ /^_:/) {
        if (!($field_index in labels)) { labels[$field_index] = "_:b" ++label_count }
        $field_index = labels[$field_index]
      }
    }
    print
  }'
}
canonicalize_d012_modified_double_lexicals() {
  # D012 的 modified DirectMapping 基线仅将两个 xsd:double 的 lexical form
  # 从科学记数法改为 decimal。按 RDF numeric value 比较，避免把 PostgreSQL
  # canonical double 输出误判为不同的 Direct Mapping graph。
  sed -E 's/"30\.0"\^\^<http:\/\/www\.w3\.org\/2001\/XMLSchema#double>/"3.0E1"^^<http:\/\/www.w3.org\/2001\/XMLSchema#double>/g; s/"20\.0"\^\^<http:\/\/www\.w3\.org\/2001\/XMLSchema#double>/"2.0E1"^^<http:\/\/www.w3.org\/2001\/XMLSchema#double>/g'
}
normalize_turtle_direct_graph() {
  # Direct Mapping 的少数基线 expected 是 Turtle（含 @base 与相对 IRI），而 CLI
  # 输出是绝对 IRI 的 N-Triples。只展开该提交资产声明的 base，保留 RDF term 和
  # blank-node 结构，再由调用方作排序/同构比较。
  awk '
    /^@base[[:space:]]+</ {
      base = $2
      sub(/^</, "", base)
      sub(/>$/, "", base)
      next
    }
    /^[[:space:]]*$/ { next }
    {
      remaining = $0
      output = ""
      while (match(remaining, /<[^>]*>/)) {
        output = output substr(remaining, 1, RSTART - 1)
        term = substr(remaining, RSTART, RLENGTH)
        iri = substr(term, 2, length(term) - 2)
        if (iri !~ /^[A-Za-z][A-Za-z0-9+.-]*:/) {
          term = "<" base iri ">"
        }
        output = output term
        remaining = substr(remaining, RSTART + RLENGTH)
      }
      print output remaining
    }
  ' "$1"
}
assert_direct_graph() {
  config=$1
  expected_file=$2
  actual=$(cd "$root" && cargo run --quiet -- query "$config" "${config%/*}/construct.rq" | sort | canonicalize_blank_nodes)
  expected=$(sort "$expected_file" | canonicalize_blank_nodes)
  assert_output "$actual" "$expected"
}
docker run -d --name "$name" -e POSTGRES_USER=rtop -e POSTGRES_PASSWORD=rtop -e POSTGRES_DB=rtop_test -p 127.0.0.1::5432 postgres:17 >/dev/null
postgres_port=$(docker port "$name" 5432/tcp | sed 's/.*://')
export RTOP_POSTGRES_PORT="$postgres_port"
until docker exec "$name" psql -U rtop -d rtop_test -c 'SELECT 1' >/dev/null 2>&1; do sleep 1; done
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-minimal/init.sql"
# 固定 Ontop 原生 OBDA mistake assets：同样的 PostgreSQL 17 datasource 下，
# SQL parser 拒绝、target reader 拒绝和可解析 source 的服务器执行失败必须区分。
# 拒绝输入均直接引用只读基线，避免由本仓库自造 mapping 替代反例。
if invalid=$(cd "$root" && cargo run --quiet -- validate tests/compat/postgres-native-obda/invalid-sql1.toml 2>&1); then
  echo "native invalid-sql1 应在加载期被拒绝" >&2
  exit 1
fi
printf '%s\n' "$invalid" | grep -F 'invalid-mapping: mapping source SQL 语法无效' >/dev/null
if invalid=$(cd "$root" && cargo run --quiet -- validate tests/compat/postgres-native-obda/missing-target.toml 2>&1); then
  echo "native missing-target-term 应在 reader 阶段被拒绝" >&2
  exit 1
fi
printf '%s\n' "$invalid" | grep -F 'invalid-mapping:' >/dev/null
if invalid=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-native-obda/valid-source-missing-relation.toml tests/compat/postgres-native-obda/runtime-source.rq 2>&1); then
  echo "native correct.obda 的缺失 PERSON relation 应由 PostgreSQL 执行期拒绝" >&2
  exit 1
fi
printf '%s\n' "$invalid" | grep -Fx 'datasource-failure: db error' >/dev/null
# RDB2RDF Direct Mapping D000：空 relation 只提供 catalog metadata，不得产生任何
# rdf:type 或 column triple。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D000/create.sql"
assert_direct_graph tests/compat/postgres-direct-d000/rtop.toml "$root/tests/compat/postgres-direct-d000/construct.expected"
# R2RML D000：原始 empty Student logical table mapping 也必须返回空 CONSTRUCT 图。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d000/rtop.toml tests/compat/postgres-d000/construct.rq)
expected=$(grep -v '^[[:space:]]*#' "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D000/mapped.nq" || true)
assert_output "$actual" "$expected"
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Student"' >/dev/null
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-d001/init.sql"
# RDB2RDF Direct Mapping D001：无主键单列表以同一 blank node 生成 class 与 column triples。
assert_direct_graph tests/compat/postgres-direct-d001/rtop.toml "$root/tests/compat/postgres-direct-d001/construct.expected"
# R2RML D001：固定基线的无主键单列 Student 表，分别验证 template IRI
# subject 与 template + rr:BlankNode subject。D018 也使用 Student 表但有不同 schema，
# 所以此处在其初始化前完成验证并清理，避免 fixture 间共享状态。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d001/iri.toml tests/compat/postgres-d001/query.rq)
assert_output "$actual" '?name="Venus" ?student=<http://example.com/Venus>'
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d001/bnode.toml tests/compat/postgres-d001/query.rq)
assert_output "$actual" '?name="Venus" ?student=_:Venus'
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Student"' >/dev/null
# RDB2RDF Direct Mapping D006：primary key 以 relation/column=value IRI 标识 row。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D006/create.sql"
assert_direct_graph tests/compat/postgres-direct-d006/rtop.toml "$root/tests/compat/postgres-direct-d006/construct.expected"
# R2RML D006：subject/predicate/object/graph 四种 constant term map 的指定具名图。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d006/rtop.toml tests/compat/postgres-d006/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D006/mappeda.nq" | sed 's# <http://example.com/graph/student> \.$# .#' | sort)
assert_output "$actual" "$expected"
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Student"' >/dev/null
# RDB2RDF Direct Mapping D002：双列无主键 relation 生成一个 blank node 的 type 与
# 两条 column property。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D002/create.sql"
assert_direct_graph tests/compat/postgres-direct-d002/rtop.toml "$root/tests/compat/postgres-direct-d002/construct.expected"
# R2RML D002a/b：多 POM+rr:class 和 template blank-node subject 分别与原始 N-Quads
# 全图对照；空白节点比较按图同构。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d002/a.toml tests/compat/postgres-d002/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D002/mappeda.nq" | sort)
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d002/b.toml tests/compat/postgres-d002/construct.rq | sort | canonicalize_blank_nodes)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D002/mappedb.nq" | sort | canonicalize_blank_nodes)
assert_output "$actual" "$expected"
# D002d/i/j：分别验证 SQL logical table 的投影 blank node、rr:sqlVersion 声明
# 与 qualified SQL column projection。全部使用基线原始 N-Quads 的完整图结果。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d002/d.toml tests/compat/postgres-d002/construct.rq | sort | canonicalize_blank_nodes)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D002/mappedd.nq" | sort | canonicalize_blank_nodes)
assert_output "$actual" "$expected"
for variant in i j; do
  actual=$(cd "$root" && cargo run --quiet -- query "tests/compat/postgres-d002/$variant.toml" tests/compat/postgres-d002/construct.rq | sort)
  expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D002/mapped$variant.nq" | sed -E 's/[[:space:]]+\.[[:space:]]*$/ ./' | sort)
  assert_output "$actual" "$expected"
done
# D002c/e/f/g/h：原始 mapping 中的未知 column/table、未引用 delimited identifier、
# 无效 SQL 和 duplicate alias 都不得产生图。g 在 mapping source 解析期拒绝；其余
# 由 PostgreSQL 17 解析原始 logical table 后稳定报告数据库语义错误，而非连接失败。
if invalid=$(cd "$root" && cargo run --quiet -- validate tests/compat/postgres-d002/g.toml 2>&1); then
  echo "D002 r2rmlg 应被拒绝" >&2
  exit 1
fi
printf '%s\n' "$invalid" | grep -F 'invalid-mapping: mapping source SQL 语法无效' >/dev/null
for variant in c e f h; do
  if invalid=$(cd "$root" && cargo run --quiet -- query "tests/compat/postgres-d002/$variant.toml" tests/compat/postgres-d002/construct.rq 2>&1); then
    echo "D002 r2rml$variant 应被 PostgreSQL 拒绝" >&2
    exit 1
  fi
  printf '%s\n' "$invalid" | grep -Fx 'datasource-failure: db error' >/dev/null
done
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Student"' >/dev/null
# RDB2RDF Direct Mapping D003：三列无主键 relation 的 type 和三个 column triples
# 必须共享同一个 blank node。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D003/create.sql"
assert_direct_graph tests/compat/postgres-direct-d003/rtop.toml "$root/tests/compat/postgres-direct-d003/construct.expected"
# R2RML D003c：两个 column template 作为 rr:Literal object 生成完整姓名。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d003/rtop.toml tests/compat/postgres-d003/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D003/mappedc.nq" | sort)
assert_output "$actual" "$expected"
if invalid=$(cd "$root" && cargo run --quiet -- validate tests/compat/postgres-d003/a.toml 2>&1); then
  echo "D003 r2rmla 应被拒绝" >&2
  exit 1
fi
printf '%s\n' "$invalid" | grep -F 'invalid-mapping: R2RML 结构错误：不支持的 rr:sqlVersion <http://www.w3.org/ns/r2rml#SQL1979>' >/dev/null
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d003/b.toml tests/compat/postgres-d003/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D003/mappedb.nq" | sed -E 's/[[:space:]]+\.[[:space:]]*$/ ./' | sort)
assert_output "$actual" "$expected"
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Student"' >/dev/null
# RDB2RDF Direct Mapping D007：单列 primary key 产生 Student/ID=10 IRI。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D007/create.sql"
assert_direct_graph tests/compat/postgres-direct-d007/rtop.toml "$root/tests/compat/postgres-direct-d007/construct.expected"
# R2RML D007b/h：直接 rr:graph IRI 的完整具名图必须和 mappedb.nq 一致；而
# column graphMap 标为 rr:Literal 必须在 mapping 载入阶段以 invalid-mapping 拒绝。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d007/b.toml tests/compat/postgres-d007/graph.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D007/mappedb.nq" | sed -E 's# <http://example.com/PersonGraph> \.[[:space:]]*$# .#; s/[[:space:]]+\.[[:space:]]*$/ ./' | sort)
assert_output "$actual" "$expected"
# D007a/c/d/g：默认图分别覆盖 rdf:type POM、多个 rr:class、两种等价的 typing
# 写法，以及显式 rr:defaultGraph。每份原始 N-Quads 都作完整图对照。
for variant in a c d g; do
  actual=$(cd "$root" && cargo run --quiet -- query "tests/compat/postgres-d007/$variant.toml" tests/compat/postgres-d007/construct.rq | sort)
  expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D007/mapped$variant.nq" | sed -E 's/^[[:space:]]+//; s/[[:space:]]+\.[[:space:]]*$/ ./' | sort)
  assert_output "$actual" "$expected"
done
# D007e/f：rr:graph 与 rr:class/rdf:type POM 的组合必须只写入 PersonGraph。
for variant in e f; do
  actual=$(cd "$root" && cargo run --quiet -- query "tests/compat/postgres-d007/$variant.toml" tests/compat/postgres-d007/graph.rq | sort)
  expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D007/mapped$variant.nq" | sed -E 's/^[[:space:]]+//; s# <http://example.com/PersonGraph> \.[[:space:]]*$# .#; s/[[:space:]]+\.[[:space:]]*$/ ./' | sort)
  assert_output "$actual" "$expected"
done
if invalid=$(cd "$root" && cargo run --quiet -- validate tests/compat/postgres-d007/h.toml 2>&1); then
  echo "D007 r2rmlh 应被拒绝" >&2
  exit 1
fi
printf '%s\n' "$invalid" | grep -F 'invalid-mapping: R2RML 结构错误：graphMap 必须是 IRI rr:constant 或 rr:template' >/dev/null
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Student"' >/dev/null
# RDB2RDF Direct Mapping D008：复合 primary key 的 Name 空格必须在 subject IRI
# component 中 percent-encode。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D008/create.sql"
assert_direct_graph tests/compat/postgres-direct-d008/rtop.toml "$root/tests/compat/postgres-direct-d008/construct.expected"
# R2RML D008a/b/c：原始三份 mapping 分别验证 template graph、无 join 的
# RefObjectMap 与一个 POM 中的多个 predicate。a 通过固定 GRAPH 模式读取
# 具名图；CONSTRUCT 的输出以相同 triples 与原始 N-Quads（去 graph context）比较。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d008/a.toml tests/compat/postgres-d008/a.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D008/mappeda.nq" | sed -E 's# <http://example.com/graph/Student/10/Venus%20Williams> \.[[:space:]]*$# .#; s/[[:space:]]+\.[[:space:]]*$/ ./' | sort)
assert_output "$actual" "$expected"
for variant in b c; do
  actual=$(cd "$root" && cargo run --quiet -- query "tests/compat/postgres-d008/$variant.toml" tests/compat/postgres-d008/construct.rq | sort)
  expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D008/mapped$variant.nq" | sed -E 's/[[:space:]]+\.[[:space:]]*$/ ./' | sort)
  assert_output "$actual" "$expected"
done
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Student"' >/dev/null
# RDB2RDF Direct Mapping D011：多对多 link table 的复合主键与两条 foreign key
# 必须分别生成 Student、Sport parent primary-key IRI。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D011/create.sql"
assert_direct_graph tests/compat/postgres-direct-d011/rtop.toml "$root/tests/compat/postgres-direct-d011/construct.expected"
# R2RML D011b：Student、Sport 与 Student_Sport 三张 physical table 分别经
# TriplesMap/LinkMap 生成多对多 ex:plays edges，完整 graph 对照 mappedb.nq。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d011/b.toml tests/compat/postgres-d011/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D011/mappedb.nq" | sort)
assert_output "$actual" "$expected"
# D011a：原始 rr:sqlQuery 在嵌套 FROM 前剥离末尾 statement terminator；多表
# view 的完整图与 mappeda.nq 比较。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d011/a.toml tests/compat/postgres-d011/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D011/mappeda.nq" | sort)
assert_output "$actual" "$expected"
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Student_Sport", "Sport", "Student"' >/dev/null
# RDB2RDF Direct Mapping D009：普通主键外键应指向 Sport IRI；NULL Sport 值不产生
# Student#Sport 或 ref-Sport triple。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D009/create.sql"
assert_direct_graph tests/compat/postgres-direct-d009/rtop.toml "$root/tests/compat/postgres-direct-d009/construct.expected"
# R2RML D009d：保留 SQL logicalTable 中命名的 COUNT("Sport") AS SPORTCOUNT，
# 在 PostgreSQL 17 上生成 numSport typed literal，并比较原始 mappedd.nq 全图。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d009/d.toml tests/compat/postgres-d009/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D009/mappedd.nq" | sed -E 's/[[:space:]]+\.[[:space:]]*$/ ./' | sort)
assert_output "$actual" "$expected"
# D009a：原始 RefObjectMap joinCondition 将 Student.Sport 与 Sport.ID 关联；
# NULL 外键不产生 practises triple，完整默认图与 mappeda.nq 比较。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d009/a.toml tests/compat/postgres-d009/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D009/mappeda.nq" | sed -E 's/[[:space:]]+\.[[:space:]]*$/ ./' | sort)
assert_output "$actual" "$expected"
# D009b：subjectMap 与 POM graph 的多个固定具名图分别查询；每个图的完整
# triples 都同原始 mappedb.nq 的对应 N-Quads 子图比较。
for graph in students practise sports; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d009/b.toml "tests/compat/postgres-d009/$graph.rq" | sort)
  expected=$(grep -F "<http://example.com/graph/$graph> ." "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D009/mappedb.nq" | tr -d '\r' | sed "s# <http://example.com/graph/$graph> .# .#" | sed -E 's/[[:space:]]+\.[[:space:]]*$/ ./' | sort)
  assert_output "$actual" "$expected"
done
# D009c 的 COUNT("Sport") 未命名且不映射为 RDF term；仍须保留 Name 的
# GROUP BY 结果，与原始 mappedc.nq 对照。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d009/c.toml tests/compat/postgres-d009/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D009/mappedc.nq" | sed -E 's/[[:space:]]+\.[[:space:]]*$/ ./' | sort)
assert_output "$actual" "$expected"
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Student", "Sport"' >/dev/null
# RDB2RDF Direct Mapping D005：无主键重复行必须保持三个不同 blank node，并以
# canonical xsd:double lexical form 表示 PostgreSQL FLOAT。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D005/create.sql"
assert_direct_graph tests/compat/postgres-direct-d005/rtop.toml "$root/tests/compat/postgres-direct-d005/construct.expected"
# D005 DirectGraph modified：基线变体仅将 xsd:double 的 3.0E1/2.0E1 改写为
# 30.0/20.0；先归一化到 Rust gate 的 scientific lexical，再比较同一个 blank-node 图。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-direct-d005/rtop.toml tests/compat/postgres-direct-d005/construct.rq | sort | canonicalize_blank_nodes)
expected=$(normalize_turtle_direct_graph "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D005/directGraph-modified.ttl" | canonicalize_d012_modified_double_lexicals | sort | canonicalize_blank_nodes)
assert_output "$actual" "$expected"
# R2RML D005b：以原始 mapping 与 PostgreSQL 表生成完整图。空白节点标签不是
# RDF graph 的稳定语义，因此在排序后按首次出现顺序 canonicalize，再做同构比较。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d005/rtop.toml tests/compat/postgres-d005/construct.rq | sort | canonicalize_blank_nodes)
expected=$(sort "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D005/mappedb.nq" | canonicalize_blank_nodes)
assert_output "$actual" "$expected"
# R2RML D005a：原始 template IRI subject、rr:class 与 FLOAT column object 的
# 默认图必须与 mappeda.nq 完整一致；重复 SQL row 在该 template 下按 RDF set 去重。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d005/a.toml tests/compat/postgres-d005/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D005/mappeda.nq" | sed -E 's/[[:space:]]+\.[[:space:]]*$/ ./' | sort)
assert_output "$actual" "$expected"
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "IOUs"' >/dev/null
# RDB2RDF Direct Mapping D012：两个无 primary key relation 的重复 row 各自保留
# blank node identity；IOUs FLOAT 使用标准 directGraph.ttl 的 xsd:double scientific
# lexical。manifest 的 modified output 仅以等价的 decimal lexical 修订该预期。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D012/create.sql"
assert_direct_graph tests/compat/postgres-direct-d012/rtop.toml "$root/tests/compat/postgres-direct-d012/construct.expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-direct-d012/rtop.toml tests/compat/postgres-direct-d012/construct.rq | sort | canonicalize_blank_nodes)
expected=$(normalize_turtle_direct_graph "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D012/directGraph-modified.ttl" | canonicalize_d012_modified_double_lexicals | sort | canonicalize_blank_nodes)
assert_output "$actual" "$expected"
# R2RML D012a/b/e：原始无主键 IOUs/Lives mapping 的重复 row blank node 应合并；
# a/e 的 FLOAT lexical 与标准基线 mappeda/mappede 作全图同构对照。
for variant in a b e; do
  actual=$(cd "$root" && cargo run --quiet -- query "tests/compat/postgres-d012/$variant.toml" tests/compat/postgres-d012/construct.rq | sort | canonicalize_blank_nodes)
  expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D012/mapped$variant.nq" | sed -E 's/[[:space:]]+\.[[:space:]]*$/ ./' | sort | canonicalize_blank_nodes)
  assert_output "$actual" "$expected"
done
# D012c/d：缺失与重复 subjectMap 都是非 conforming R2RML mapping，须在加载期
# 给出 mapping 类错误，不能被混同为 PostgreSQL datasource failure。
if invalid=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d012/c.toml tests/compat/postgres-d012/construct.rq 2>&1); then
  printf '%s\n' "$invalid" >&2
  exit 1
fi
printf '%s\n' "$invalid" | grep -F '缺少 rr:subjectMap' >/dev/null
if invalid=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d012/d.toml tests/compat/postgres-d012/construct.rq 2>&1); then
  printf '%s\n' "$invalid" >&2
  exit 1
fi
printf '%s\n' "$invalid" | grep -F '只能有一个 rr:subjectMap' >/dev/null
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Lives", "IOUs"' >/dev/null
# RDB2RDF Direct Mapping D010：relation/column identifier 的空格是 IRI 固定片段，
# 必须 percent-encode，同时仍使用原始 PostgreSQL quoted identifier 查询。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D010/create.sql"
assert_direct_graph tests/compat/postgres-direct-d010/rtop.toml "$root/tests/compat/postgres-direct-d010/construct.expected"
# R2RML D010a：带空格的 quoted table/column identifier 作为 template 槽位时，
# 三个 Country Code subject 和 name literal 必须和 mappeda.nq 完整一致。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d010/a.toml tests/compat/postgres-d010/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D010/mappeda.nq" | sed -E 's/[[:space:]]+\.[[:space:]]*$/ ./' | sort)
assert_output "$actual" "$expected"
# R2RML D010b：多列 IRI template 对 PostgreSQL quoted identifier 的值做 RFC3986
# component encoding；完整 graph 与原始 mappedb.nq 比较。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d010/b.toml tests/compat/postgres-d010/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D010/mappedb.nq" | sort)
assert_output "$actual" "$expected"
# D010c：literal template 中的 \\{ / \\} 是固定花括号而非 column 槽位；完整图
# 与原始 mappedc.nq 比较。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d010/c.toml tests/compat/postgres-d010/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D010/mappedc.nq" | sort)
assert_output "$actual" "$expected"
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Country Info"' >/dev/null
# RDB2RDF Direct Mapping D015：多行复合 primary key 按 catalog key 顺序生成
# Country/Code=...;Lan=... IRI。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D015/create.sql"
assert_direct_graph tests/compat/postgres-direct-d015/rtop.toml "$root/tests/compat/postgres-direct-d015/construct.expected"
# R2RML D015a/b：两个 SQL logicalTable 分别生成 @en/@es language literals；非法
# rr:language 必须在加载 mapping 时稳定归类，而非等到 datasource 连接失败。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d015/a.toml tests/compat/postgres-d015/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D015/mappeda.nq" | sort)
assert_output "$actual" "$expected"
if invalid=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d015/b.toml tests/compat/postgres-d015/construct.rq 2>&1); then
  printf '%s\n' "$invalid" >&2
  exit 1
fi
printf '%s\n' "$invalid" | grep -F 'invalid-mapping: R2RML 结构错误：rr:language 不是有效的 BCP47 语言标签' >/dev/null
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Country"' >/dev/null
# RDB2RDF Direct Mapping D013：NULL column 不产生 property triple，row 的 primary-key
# IRI、type 和其它 non-NULL columns 必须不受影响。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D013/create.sql"
assert_direct_graph tests/compat/postgres-direct-d013/rtop.toml "$root/tests/compat/postgres-direct-d013/construct.expected"
# R2RML D013：原始 Person mapping 的 subject template 含 DateOfBirth；NULL 行不得
# 产生不完整 IRI 或任何 triple，仅保留与 mappeda.nq 相同的完整图。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d013/rtop.toml tests/compat/postgres-d013/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D013/mappeda.nq" | sort)
assert_output "$actual" "$expected"
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Person"' >/dev/null
# R2RML D019a/b：带 @base 的 subject rr:column 对相对 Carlos 做 IRI resolution；
# tableName 全表 variant 会遇到含空格的 Juan Daniel，须归类为 R2RML data error。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D019/create.sql"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d019/a.toml tests/compat/postgres-d019/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D019/mappeda.nq" | sort)
assert_output "$actual" "$expected"
if invalid=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d019/b.toml tests/compat/postgres-d019/construct.rq 2>&1); then
  printf '%s\n' "$invalid" >&2
  exit 1
fi
printf '%s\n' "$invalid" | grep -F 'R2RML data error：IRI 值无效' >/dev/null
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Employee"' >/dev/null
# R2RML D020a/b：IRI template 必须 percent-encode column component；直接以
# rr:column rr:IRI 读取空格、斜杠等原始值时应稳定报告 R2RML data error。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D020/create.sql"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d020/a.toml tests/compat/postgres-d020/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D020/mappeda.nq" | sort)
assert_output "$actual" "$expected"
if invalid=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d020/b.toml tests/compat/postgres-d020/construct.rq 2>&1); then
  printf '%s\n' "$invalid" >&2
  exit 1
fi
printf '%s\n' "$invalid" | grep -F 'R2RML data error：IRI 值无效' >/dev/null
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Student"' >/dev/null
# R2RML D026：Student→Sport→SportType 的两级 RefObjectMap 各使用不同 child/parent
# column 名；NULL Student.Sport 不产生 practises edge，完整图与 mappeda.nq 对照。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D026/create.sql"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d026/rtop.toml tests/compat/postgres-d026/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D026/mappeda.nq" | sort)
assert_output "$actual" "$expected"
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Student", "Sport", "SportType"' >/dev/null
# R2RML D016a-d：基线 create.sql 的 VARBINARY/X literal 非 PostgreSQL 方言；以
# 等价 PostgreSQL 17 schema 初始化同三条非 Photo row，直接加载原始 mapping 并逐图对照。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-d016/r2rml-init.sql"
for variant in a b c d e; do
  actual=$(cd "$root" && cargo run --quiet -- query "tests/compat/postgres-d016/$variant.toml" tests/compat/postgres-d016/construct.rq | sort)
  expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D016/mapped$variant.nq" | sort)
  assert_output "$actual" "$expected"
done
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Patient"' >/dev/null
# RDB2RDF Direct Mapping D016：PostgreSQL REAL/FLOAT、date、timestamp、boolean 和
# bytea（由基线 VARBINARY/X literal 做方言等价转换）产生完整 typed RDF terms。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-direct-d016/init.sql"
assert_direct_graph tests/compat/postgres-direct-d016/rtop.toml "$root/tests/compat/postgres-direct-d016/construct.expected"
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Patient"' >/dev/null
# RDB2RDF Direct Mapping D014：parent 无 primary key 但被 UNIQUE 外键引用时，
# child ref triple 必须指向该 parent row 的同一 blank node，而非伪造 IRI。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D014/create.sql"
assert_direct_graph tests/compat/postgres-direct-d014/rtop.toml "$root/tests/compat/postgres-direct-d014/construct.expected"
# R2RML D014a：rr:column blank-node subject 与 inverseExpression 的完整默认图；
# blank node label 按图同构规范化后同 mappeda.nq 对照。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d014/a.toml tests/compat/postgres-d014/construct.rq | sort | canonicalize_blank_nodes)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D014/mappeda.nq" | sed -E 's/[[:space:]]+\.[[:space:]]*$/ ./' | sort | canonicalize_blank_nodes)
assert_output "$actual" "$expected"
# D014b/c：两个 TriplesMap 的 named/inline object map、常量、datatype 与
# RefObjectMap join 都必须共享 Department10 blank node；全图按同构对照。
for variant in b c; do
  actual=$(cd "$root" && cargo run --quiet -- query "tests/compat/postgres-d014/$variant.toml" tests/compat/postgres-d014/construct.rq | sort | canonicalize_blank_nodes)
  expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D014/mapped$variant.nq" | sed -E 's/[[:space:]]+\.[[:space:]]*$/ ./' | sort | canonicalize_blank_nodes)
  assert_output "$actual" "$expected"
done
# D014d：原始 CASE role 翻译表 SQL logicalTable 必须产生 general-office role IRI。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d014/d.toml tests/compat/postgres-d014/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D014/mappedd.nq" | sort)
assert_output "$actual" "$expected"
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "LIKES", "EMP", "DEPT"' >/dev/null
# RDB2RDF Direct Mapping D021：复合外键任一值为 NULL 时不产生 ref triple；完整
# 非 NULL 键则经 parent UNIQUE key JOIN 指向 parent primary-key IRI。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D021/create.sql"
assert_direct_graph tests/compat/postgres-direct-d021/rtop.toml "$root/tests/compat/postgres-direct-d021/construct.expected"
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Source", "Target"' >/dev/null
# RDB2RDF Direct Mapping D023：外键可引用 parent 的 UNIQUE key；ref object 仍须
# 使用 parent primary-key IRI，因此 planner 通过 JOIN 投影 parent PK。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D023/create.sql"
assert_direct_graph tests/compat/postgres-direct-d023/rtop.toml "$root/tests/compat/postgres-direct-d023/construct.expected"
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Source", "Target"' >/dev/null
# RDB2RDF Direct Mapping D022：无 primary key 的 parent 由复合 UNIQUE key 引用；
# child reference 必须与 Target row 使用同一 blank node identity。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D022/create.sql"
assert_direct_graph tests/compat/postgres-direct-d022/rtop.toml "$root/tests/compat/postgres-direct-d022/construct.expected"
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Source", "Target"' >/dev/null
# RDB2RDF Direct Mapping D024：parent UNIQUE key 具有 NULL 时，完整 child key 仍可
# 指向其它 parent PK；partial-NULL child key 不产生 reference。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D024/create.sql"
assert_direct_graph tests/compat/postgres-direct-d024/rtop.toml "$root/tests/compat/postgres-direct-d024/construct.expected"
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Source", "Target"' >/dev/null
# RDB2RDF Direct Mapping D025：五 relation、多组 primary/non-primary 外键和无主键
# Projects parent；TaskAssignments 的复合外键必须指向同一个 Projects blank node。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D025/create.sql"
assert_direct_graph tests/compat/postgres-direct-d025/rtop.toml "$root/tests/compat/postgres-direct-d025/construct.expected"
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "TaskAssignments", "Projects", "Department", "People", "Addresses"' >/dev/null
# RDB2RDF Direct Mapping D017：由 PostgreSQL catalog 原生读取 Unicode relation/column、
# 复合主键及外键；完整 term 比较包含无主键 relation 的 blank node identity。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-direct-d017/init.sql"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-direct-d017/rtop.toml tests/compat/postgres-direct-d017/query.rq)
expected=$(cat "$root/tests/compat/postgres-direct-d017/expected.txt")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-direct-d017/rtop.toml tests/compat/postgres-direct-d017/construct.rq | sed 's/_:[^ ]*/_:a/g' | sort)
expected=$(sort "$root/tests/compat/postgres-direct-d017/construct.expected")
assert_output "$actual" "$expected"
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "成分", "植物"' >/dev/null
# RDB2RDF Direct Mapping D018：无主键 CHAR(15) column 的 literal 必须保留 PostgreSQL
# fixed-width 尾部空格，且三行仍保持不同 blank node identity。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D018/create.sql"
assert_direct_graph tests/compat/postgres-direct-d018/rtop.toml "$root/tests/compat/postgres-direct-d018/construct.expected"
# R2RML D018a：使用同一份基线三行 CHAR(15) schema；完整默认图必须保留每行
# Name literal 的 fixed-width 尾部空格，并与 mappeda.nq 一致。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d018/rtop.toml tests/compat/postgres-d018/query.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D018/mappeda.nq" | sed -E 's/[[:space:]]+\.[[:space:]]*$/ ./' | sort)
assert_output "$actual" "$expected"
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE "Student"' >/dev/null
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-d016/init.sql"
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D004/create.sql"
# RDB2RDF Direct Mapping D004：无主键双列表生成 relation type 与两个 column properties。
assert_direct_graph tests/compat/postgres-direct-d004/rtop.toml "$root/tests/compat/postgres-direct-d004/construct.expected"
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-query-kinds/init.sql"
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-order-by/init.sql"
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-datatype-manifest/init.sql"
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-expressions/init.sql"
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-aggregates/init.sql"
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-constraints/init.sql"
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-metamapping/epnet-init.sql"
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-annotation/movie-init.sql"
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-nested/init.sql"
(
  cd "$root"
  RTOP_POSTGRES_HOST=127.0.0.1 RTOP_POSTGRES_PORT="$postgres_port" RTOP_POSTGRES_DATABASE=rtop_test \
    RTOP_POSTGRES_USER=rtop RTOP_POSTGRES_PASSWORD=rtop cargo test --locked --test postgres_adapter
)

# 固定 UniversityTBoxFactTest 的 PostgreSQL 可观察子集：原始 university.obda
# 由真实 adapter 读取；本地 root ontology 以相对 owl:imports 引入固定 complete
# TBox。subclass、facts 的 domain/range、mapping subProperty 与 inverse 都必须在
# 同一 PostgreSQL 17 runtime 中给出 RDF term；互斥 type 则在构建期稳定拒绝。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/../ontop/binding/rdf4j/src/test/resources/tbox-facts/university.sql"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-ontology/university-mapping.toml tests/compat/postgres-ontology/subclass-imported.rq)
expected=$(cat "$root/tests/compat/postgres-ontology/subclass-imported.expected")
assert_output "$actual" "$expected"
for query in domain-fact range-fact; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-ontology/university.toml "tests/compat/postgres-ontology/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-ontology/$query.expected")
  assert_output "$actual" "$expected"
done
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-ontology/teaching.toml tests/compat/postgres-ontology/subproperty.rq | sed 's/?teacher=\([^ ]*\) ?course=\(.*\)/?course=\2 ?teacher=\1/' | sort)
expected=$(cat "$root/tests/compat/postgres-ontology/teaching.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-ontology/teaching.toml tests/compat/postgres-ontology/inverse.rq)
expected=$(cat "$root/tests/compat/postgres-ontology/teaching.expected")
assert_output "$actual" "$expected"
if (cd "$root" && cargo run --quiet -- query tests/compat/postgres-ontology/inconsistent.toml tests/compat/postgres-ontology/domain-fact.rq >/dev/null 2>"$scratch/university-inconsistent.stderr"); then
  exit 1
fi
grep -F 'ontology inconsistent' "$scratch/university-inconsistent.stderr" >/dev/null

# 固定 FactsFileTest 的三种 facts 格式必须与 PostgreSQL mapping 进入同一
# VkgRuntime：Turtle 结果同时含两个数据库 row 和一个 typed facts literal；
# N-Quads 保留 named graph；RDF/XML 接受显式 facts base。格式和 I/O 失败在
# runtime 建立期稳定分类为 invalid-facts。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-facts/init.sql"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-facts/turtle.toml tests/compat/postgres-facts/companies.rq)
expected=$(cat "$root/tests/compat/postgres-facts/companies.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-facts/nquads.toml tests/compat/postgres-facts/named-graph.rq)
expected=$(cat "$root/tests/compat/postgres-facts/named-graph.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-facts/rdfxml.toml tests/compat/postgres-facts/rdfxml-company.rq)
expected=$(cat "$root/tests/compat/postgres-facts/rdfxml-company.expected")
assert_output "$actual" "$expected"
for config in missing-facts malformed-facts; do
  if (cd "$root" && cargo run --quiet -- query "tests/compat/postgres-facts/$config.toml" tests/compat/postgres-facts/companies.rq >/dev/null 2>"$scratch/$config.stderr"); then
    exit 1
  fi
  grep -F 'invalid-facts:' "$scratch/$config.stderr" >/dev/null
done
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE company' >/dev/null
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-minimal/rtop.toml tests/compat/postgres-minimal/query.rq)
expected=$(cat "$root/tests/compat/postgres-minimal/expected.txt")
assert_output "$actual" "$expected"

# R2RML D004：原始两个 TriplesMap 的完整默认图必须与 mappeda.nq 一致；原始
# r2rmlb.ttl 将 rr:Literal 用于 subjectMap，故必须在 mapping 载入期被拒绝。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d004/rtop.toml tests/compat/postgres-d004/construct.rq | sort)
expected=$(tr -d '\r' < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D004/mappeda.nq" | sed -E 's/[[:space:]]+\.[[:space:]]*$/ ./' | sort)
assert_output "$actual" "$expected"
if (cd "$root" && cargo run --quiet -- validate tests/compat/postgres-d004/invalid.toml >/dev/null 2>"$scratch/d004-invalid.stderr"); then
  exit 1
fi
grep -q 'R2RML.*subject.*Literal\|subject.*Literal' "$scratch/d004-invalid.stderr"


# 固定 AbstractNestedDataTest：直接从 PostgreSQL JSON/JSONB/array 原始列展开，
# 逐种输入保留 position、NULL、二维数组和 RDF triple identity 的可观察结果。
assert_nested_rows() {
  config=$1
  query=$2
  expected_rows=$3
  rows=$(cd "$root" && cargo run --quiet -- query "tests/compat/postgres-nested/$config.toml" "tests/compat/postgres-nested/$query.rq" | sed '/^$/d' | wc -l | tr -d ' ')
  [ "$rows" = "$expected_rows" ]
}
for config in rtop json array; do
  for item in \
    'flatten-index 7' 'flatten-dates 7' 'flatten-income 7' 'flatten-workers 11' \
    'flatten-empty 2' 'flatten-first-name 7' 'flatten-age 5' 'spo 89'; do
    set -- $item
    assert_nested_rows "$config" "$1" "$2"
  done
  actual=$(cd "$root" && cargo run --quiet -- query "tests/compat/postgres-nested/$config.toml" tests/compat/postgres-nested/flatten-aggregate.rq | sort)
  expected=$(cat "$root/tests/compat/postgres-nested/flatten-aggregate.expected")
  assert_output "$actual" "$expected"
done

# 固定 CastPostgreSQLTest 的 PostgreSQL lexical override：double 0 转 float 为 0。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml tests/compat/postgres-cast/cast-float-from-double.rq)
expected=$(cat "$root/tests/compat/postgres-cast/cast-float-from-double.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml tests/compat/postgres-cast/cast-double-from-integer.rq)
expected=$(cat "$root/tests/compat/postgres-cast/cast-double-from-integer.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml tests/compat/postgres-cast/cast-float-from-boolean.rq)
expected=$(cat "$root/tests/compat/postgres-cast/cast-float-from-boolean.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml tests/compat/postgres-cast/cast-float-from-decimal.rq)
expected=$(cat "$root/tests/compat/postgres-cast/cast-float-from-decimal.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml tests/compat/postgres-cast/cast-decimal-from-string.rq)
expected=$(cat "$root/tests/compat/postgres-cast/cast-decimal-from-string.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml tests/compat/postgres-cast/cast-double-from-boolean.rq)
expected=$(cat "$root/tests/compat/postgres-cast/cast-double-from-boolean.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml tests/compat/postgres-cast/cast-float-from-string.rq)
expected=$(cat "$root/tests/compat/postgres-cast/cast-float-from-string.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml tests/compat/postgres-cast/cast-float-from-invalid-string.rq)
expected=$(cat "$root/tests/compat/postgres-cast/cast-float-from-invalid-string.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml tests/compat/postgres-cast/cast-double-from-float.rq)
expected=$(cat "$root/tests/compat/postgres-cast/cast-double-from-float.expected")
assert_output "$actual" "$expected"
for query in cast-float-from-integer cast-double-from-string-integer cast-decimal-from-boolean; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml "tests/compat/postgres-cast/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-cast/$query.expected")
  assert_output "$actual" "$expected"
done
for query in cast-integer-from-float-negative cast-integer-from-string cast-integer-from-boolean; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml "tests/compat/postgres-cast/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-cast/$query.expected")
  assert_output "$actual" "$expected"
done
for query in cast-boolean-from-float cast-boolean-from-invalid-string cast-string-from-datetime; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml "tests/compat/postgres-cast/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-cast/$query.expected")
  assert_output "$actual" "$expected"
done
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-cast/books.toml tests/compat/postgres-cast/cast-double-from-mapped-decimal.rq)
expected=$(cat "$root/tests/compat/postgres-cast/cast-double-from-mapped-decimal.expected")
assert_output "$actual" "$expected"
for query in cast-float-from-mapped-decimal cast-decimal-from-mapped-decimal cast-integer-from-mapped-decimal; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-cast/books.toml "tests/compat/postgres-cast/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-cast/$query.expected")
  assert_output "$actual" "$expected"
done
for query in cast-boolean-from-mapped-decimal cast-string-from-mapped-decimal; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-cast/books.toml "tests/compat/postgres-cast/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-cast/$query.expected")
  assert_output "$actual" "$expected"
done
for query in cast-date-from-datetime cast-date-from-invalid-string cast-datetime-from-datetime; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml "tests/compat/postgres-cast/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-cast/$query.expected")
  assert_output "$actual" "$expected"
done
for query in cast-double-from-double cast-float-from-float cast-decimal-from-decimal; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml "tests/compat/postgres-cast/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-cast/$query.expected")
  assert_output "$actual" "$expected"
done
for query in cast-boolean-direct cast-string-direct; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml "tests/compat/postgres-cast/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-cast/$query.expected")
  assert_output "$actual" "$expected"
done
for query in cast-decimal-direct cast-integer-direct; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml "tests/compat/postgres-cast/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-cast/$query.expected")
  assert_output "$actual" "$expected"
done
for query in cast-string-iri-and-literal cast-date-direct cast-date-from-integer; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml "tests/compat/postgres-cast/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-cast/$query.expected")
  assert_output "$actual" "$expected"
done
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml tests/compat/postgres-cast/cast-invalid-direct.rq)
expected=$(cat "$root/tests/compat/postgres-cast/cast-invalid-direct.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml tests/compat/postgres-cast/cast-float-and-double-from-string.rq)
expected=$(cat "$root/tests/compat/postgres-cast/cast-float-and-double-from-string.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-cast/books.toml tests/compat/postgres-cast/cast-mapped-title-invalid.rq)
expected=$(cat "$root/tests/compat/postgres-cast/cast-mapped-title-invalid.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-metamapping/epnet.toml tests/compat/postgres-metamapping/query.rq)
expected=$(cat "$root/tests/compat/postgres-metamapping/expected.txt")
assert_output "$actual" "$expected"
for query in descriptions dates gross genre actor vip company-id id-title; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-annotation/movie.toml "tests/compat/postgres-annotation/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-annotation/$query.expected")
  assert_output "$actual" "$expected"
done
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-annotation/movie-ontology.toml tests/compat/postgres-annotation/ontology-label.rq)
rows=$(printf '%s\n' "$actual" | rg -c '^\?r=' || true)
[ "$rows" = "4" ]
# `postgres-datatype-manifest` 已用同名表验证标识符语义；DOID fixture 需要其
# 自己的 NOT NULL schema 和 76 行输入，因而在两项独立断言之间清除该表。
docker exec "$name" psql -U rtop -d rtop_test -c 'DROP TABLE tb_books' >/dev/null
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-annotation/doid-init.sql"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-annotation/doid.toml tests/compat/postgres-annotation/doid-comment.rq)
rows=$(printf '%s\n' "$actual" | rg -c '^\?x=<http://purl\.obolibrary\.org/obo/DOID_0060309>$' || true)
[ "$rows" = "76" ]
# DOID fixture 的 tb_books 与后续 LowercaseIdentifier fixture 共用表名；DOID
# 断言完成后移除其 76 条专属输入，并恢复后者所需的唯一 seed。
docker exec "$name" psql -U rtop -d rtop_test -c "DELETE FROM tb_books WHERE bk_title = 'NT MGI.'" >/dev/null
docker exec "$name" psql -U rtop -d rtop_test -c "INSERT INTO tb_books (bk_title) VALUES ('a')" >/dev/null

# 固定 Ontop DistinctInAggregatePostgresTest 的四个 GROUP BY/DISTINCT aggregate 查询。
for query in sum-distinct avg-distinct count-distinct group-concat-distinct; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/rtop.toml "tests/compat/postgres-aggregates/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-aggregates/$query.expected")
  assert_output "$actual" "$expected"
done
for query in group-concat-language group-concat-all; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/gconcat.toml "tests/compat/postgres-aggregates/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-aggregates/$query.expected")
  assert_output "$actual" "$expected"
done
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/min-max.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/min-max.expected")
assert_output "$actual" "$expected"
for query in min-students-2 max-students-2; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml "tests/compat/postgres-aggregates/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-aggregates/$query.expected")
  assert_output "$actual" "$expected"
done
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/limit-subquery-1.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/limit-subquery-1.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/empty-numeric-aggregates.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/empty-numeric-aggregates.expected")
assert_output "$actual" "$expected"
# 固定 Ontop AbstractLeftJoinProfTest 的 SUM/AVG 分组与 OPTIONAL 空组查询。
for query in sum-students-1 sum-students-2 sum-students-3 avg-students-1 avg-students-2 avg-students-3; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml "tests/compat/postgres-aggregates/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-aggregates/$query.expected")
  assert_output "$actual" "$expected"
done
for query in sum-students-4 sum-students-5; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml "tests/compat/postgres-aggregates/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-aggregates/$query.expected")
  assert_output "$actual" "$expected"
done
for query in duration-1 multityped-sum-1 multityped-avg-1; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml "tests/compat/postgres-aggregates/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-aggregates/$query.expected")
  assert_output "$actual" "$expected"
done
# 固定 AbstractLeftJoinProfTest 的 GROUP BY（无 aggregate）去重投影。
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/distinct-as-group-by.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/distinct-as-group-by.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/group-concat-optional.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/group-concat-optional.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/group-concat-optional-union.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/group-concat-optional-union.expected")
assert_output "$actual" "$expected"
for query in group-concat-optional-union-distinct group-concat-optional-union-distinct-separator; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml "tests/compat/postgres-aggregates/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-aggregates/group-concat-optional.expected")
  assert_output "$actual" "$expected"
done
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/group-concat-optional-union-separator.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/group-concat-optional-union-separator.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/group-concat-coalesce.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/group-concat-coalesce.expected")
assert_output "$actual" "$expected"
for query in optional-unbound-nickname optional-unbound-bind optional-unbound-lastname; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml "tests/compat/postgres-aggregates/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-aggregates/$query.expected")
  assert_output "$actual" "$expected"
done
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/optional-required-teacher-nickname.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/optional-required-teacher-nickname.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/simple-first-name.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/simple-first-name.expected")
assert_output "$actual" "$expected"
for query in optional-full-name-single optional-full-name-chained; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml "tests/compat/postgres-aggregates/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-aggregates/optional-full-name.expected")
  assert_output "$actual" "$expected"
done
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/optional-firstname-nickname.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/optional-firstname-nickname.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/optional-simple-nickname.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/optional-simple-nickname.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/optional-nickname-course.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/optional-nickname-course.expected")
assert_output "$actual" "$expected"
for query in course-teacher-name course-join-left-1; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml "tests/compat/postgres-aggregates/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-aggregates/course-teacher-name.expected")
  assert_output "$actual" "$expected"
done
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/course-join-left-2.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/course-join-left-2.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/optional-not-eq-or-unbound.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/optional-not-eq-or-unbound.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/optional-preferences.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/optional-preferences.expected")
assert_output "$actual" "$expected"
for query in optional-useless-right optional-teaches-at; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml "tests/compat/postgres-aggregates/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-aggregates/$query.expected")
  assert_output "$actual" "$expected"
done
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/optional-teacher-id.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/optional-teacher-id.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/non-optimizable-left-join-mix.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/non-optimizable-left-join-mix.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/values-node-ontology-property.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/values-node-ontology-property.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/aggregation-mapping-prof-student-count-property.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/aggregation-mapping-prof-student-count-property.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml tests/compat/postgres-aggregates/properties.rq)
expected=$(cat "$root/tests/compat/postgres-aggregates/properties.expected")
assert_output "$actual" "$expected"
for query in minus-multityped-sum minus-multityped-avg; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-aggregates/redundant-join.toml "tests/compat/postgres-aggregates/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-aggregates/minus-multityped-sum.expected")
  assert_output "$actual" "$expected"
done

# 固定 Ontop RegexPostgresSQLTest 的 source SQL，由 PostgreSQL 执行 `~*` 与 `!~*`。
for query_and_rows in 'regex-address 2' 'regex-person 3'; do
  set -- $query_and_rows
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml "tests/compat/postgres-expressions/$1.rq")
  rows=$(printf '%s\n' "$actual" | sed '/^$/d' | wc -l | tr -d ' ')
  [ "$rows" = "$2" ]
done
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/replace.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/replace.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/numeric-functions.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/numeric-functions.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/numeric-arithmetic.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/numeric-arithmetic.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/datetime-functions.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/datetime-functions.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/sha256.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/sha256.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex-expression.toml tests/compat/postgres-expressions/regex-expression.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/regex-expression.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/ofn-between.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/ofn-between.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/ofn-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/ofn-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/ofn-millis-between.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/ofn-millis-between.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/ofn-date-between.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/ofn-date-between.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/datetime-extractors-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/datetime-extractors-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/bound-optional-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/bound-optional-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/term-predicates-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/term-predicates-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/lang-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/lang-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/datatype-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/datatype-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/regex-optional-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/regex-optional-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/rdf-term-equal-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/rdf-term-equal-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/rdf-term-str-not-equal-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/rdf-term-str-not-equal-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/same-term-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/same-term-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/numeric-functions-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/numeric-functions-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/sha256-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/sha256-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/strstarts-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/strstarts-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/str-before-after-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/str-before-after-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/contains-bind-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/contains-bind-mapping-input.expected")
assert_output "$actual" "$expected"
for query in logical-and-mapping-input logical-and-distinct-mapping-input logical-or-mapping-input; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml "tests/compat/postgres-expressions/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-expressions/$query.expected")
  assert_output "$actual" "$expected"
done
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-expressions/timezone-books.sql"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/timezone-mapping.toml tests/compat/postgres-expressions/str-timezone-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/str-timezone-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/timezone-mapping.toml tests/compat/postgres-expressions/tz-timezone-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/tz-timezone-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/divide-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/divide-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/concat-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/concat-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/replace-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/replace-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/substr-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/substr-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/encode-for-uri-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/encode-for-uri-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/strlen-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/strlen-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/contains-filter-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/contains-filter-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/strends-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/strends-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/langmatches-optional-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/langmatches-optional-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/ofn-mapping.toml tests/compat/postgres-expressions/case-concat-mapping-input.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/case-concat-mapping-input.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/now.rq)
printf '%s\n' "$actual" | rg -q '^\?now=".+"\^\^<http://www\.w3\.org/2001/XMLSchema#dateTime>$'
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/random-identifiers.rq)
printf '%s\n' "$actual" | rg -q '\?uuid=<urn:uuid:[0-9a-f-]{36}>'
printf '%s\n' "$actual" | rg -q '\?struuid="[0-9a-f-]{36}"\^\^<http://www\.w3\.org/2001/XMLSchema#string>'
printf '%s\n' "$actual" | rg -q '\?rand="(0|0\.[0-9]+|1)"\^\^<http://www\.w3\.org/2001/XMLSchema#decimal>'
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/values.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/values.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex-expression.toml tests/compat/postgres-expressions/optional.rq)
rows=$(printf '%s\n' "$actual" | rg -o '\?person=' | wc -l | tr -d ' ')
[ "$rows" = "4" ]
printf '%s\n' "$actual" | rg -q '\?ssn="two"'
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/values-union.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/values-union.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex-expression.toml tests/compat/postgres-expressions/union-bgp.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/union-bgp.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/subquery-distinct.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/subquery-distinct.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/subquery-limit.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/subquery-limit.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/subquery-union-bind.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/subquery-union-bind.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/logical.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/logical.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/rdf-term-predicates.rq)
printf '%s\n' "$actual" | rg -q '^\?bound="false"\^\^<http://www\.w3\.org/2001/XMLSchema#boolean> \?iri="true".*\?literal="false".*\?numeric="false".*\?value=<https://example\.test/a>$'
printf '%s\n' "$actual" | rg -q '^\?bound="false".*\?iri="false".*\?literal="true".*\?numeric="true".*\?value="2"\^\^<http://www\.w3\.org/2001/XMLSchema#integer>$'
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/lang.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/lang.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/datatype.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/datatype.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/comparison.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/comparison.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/same-term.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/same-term.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/not-expression.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/not-expression.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/iri-uri.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/iri-uri.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/iri-uri-relative.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/iri-uri-relative.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/iri-union-values.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/iri-union-values.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/if-coalesce.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/if-coalesce.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/bnode.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/bnode.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/coalesce-arithmetic-errors.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/coalesce-arithmetic-errors.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/numeric-promotion.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/numeric-promotion.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/langmatches.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/langmatches.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/tz.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/tz.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/encode-for-uri.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/encode-for-uri.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/substr-language.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/substr-language.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/language-comparison.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/language-comparison.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/case-language.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/case-language.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/str-iri.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/str-iri.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/str-before-after-language.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/str-before-after-language.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-expressions/regex.toml tests/compat/postgres-expressions/str-before-after-empty.rq)
expected=$(cat "$root/tests/compat/postgres-expressions/str-before-after-empty.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-query-kinds/rtop-null.toml tests/compat/postgres-query-kinds/null-construct.rq)
expected=$(cat "$root/tests/compat/postgres-query-kinds/null-construct.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-query-kinds/rtop-describe-compliance.toml tests/compat/postgres-query-kinds/describe-compliance.rq)
expected=$(cat "$root/tests/compat/postgres-query-kinds/describe-compliance.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-query-kinds/rtop-construct-compliance.toml tests/compat/postgres-query-kinds/construct-compliance.rq)
expected=$(cat "$root/tests/compat/postgres-query-kinds/construct-compliance.expected")
assert_output "$actual" "$expected"
for kind in ask construct describe; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-query-kinds/rtop.toml "tests/compat/postgres-query-kinds/$kind.rq")
  expected=$(cat "$root/tests/compat/postgres-query-kinds/$kind.expected")
  assert_output "$actual" "$expected"
done
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-query-kinds/rtop-compliance.toml tests/compat/postgres-query-kinds/ask-compliance.rq)
expected=$(cat "$root/tests/compat/postgres-query-kinds/ask-compliance.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-d016/rtop.toml tests/compat/postgres-d016/query.rq)
expected=$(cat "$root/tests/compat/postgres-d016/expected.txt")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-order-by/rtop.toml tests/compat/postgres-order-by/query.rq)
expected=$(cat "$root/tests/compat/postgres-order-by/expected.txt")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-datatype-manifest/rtop.toml tests/compat/postgres-datatype-manifest/query.rq)
expected=$(cat "$root/tests/compat/postgres-datatype-manifest/expected.txt")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-datatype-manifest/rtop.toml tests/compat/postgres-datatype-manifest/numeric-filter.rq)
expected=$(cat "$root/tests/compat/postgres-datatype-manifest/numeric-filter.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-datatype-manifest/rtop.toml tests/compat/postgres-datatype-manifest/character-bgp.rq)
expected=$(cat "$root/tests/compat/postgres-datatype-manifest/character-bgp.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-datatype-manifest/rtop.toml tests/compat/postgres-datatype-manifest/string-cast-filter.rq)
expected=$(cat "$root/tests/compat/postgres-datatype-manifest/string-cast-filter.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-datatype-manifest/rtop.toml tests/compat/postgres-datatype-manifest/interval-string-filter.rq)
expected=$(cat "$root/tests/compat/postgres-datatype-manifest/interval-string-filter.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-datatype-manifest/rtop.toml tests/compat/postgres-datatype-manifest/manifest-columns.rq)
expected=$(cat "$root/tests/compat/postgres-datatype-manifest/manifest-columns.expected")
assert_output "$actual" "$expected"

# 直接运行固定 Ontop PostgreSQL datatype manifest 的所有可执行 .rq。其 result .ttl
# 都只断言 rsi:size；general/Type: all 被 PgsqlDatatypeTest.parameters() 明确忽略。
assert_baseline_rows() {
  config=$1
  query=$2
  expected_rows=$3
  actual=$(cd "$root" && cargo run --quiet -- query "$config" "$root/../ontop/test/docker-tests/src/test/resources/testcases-docker/$query")
  rows=$(printf '%s\n' "$actual" | sed '/^$/d' | wc -l | tr -d ' ')
  [ "$rows" = "$expected_rows" ]
}

for query in boolean/boolean; do
  assert_baseline_rows tests/compat/postgres-datatype-manifest/baseline-boolean.toml "$query.rq" 1
done
for query in character/char character/varchar character/text character/character character/name character/char-graph character/varchar-graph character/text-graph character/character-graph character/name-graph; do
  assert_baseline_rows tests/compat/postgres-datatype-manifest/baseline-character.toml "$query.rq" 1
done
for query in numeric/integer numeric/smallint numeric/bigint numeric/numeric numeric/real numeric/double numeric/serial numeric/bigserial; do
  assert_baseline_rows tests/compat/postgres-datatype-manifest/baseline-numeric.toml "$query.rq" 1
done
for query in dateLiteral date date-str date-bgp timeLiteral time time-str time-bgp time_tz time_tz_Literal time_tz-bgp timestamp timestamp-str timestamp_tz timestamp_tz-str; do
  expected_rows=1
  case "$query" in
    dateLiteral|timeLiteral|time_tz_Literal) expected_rows=0 ;;
  esac
  assert_baseline_rows tests/compat/postgres-datatype-manifest/baseline-datetime.toml "datetime/$query.rq" "$expected_rows"
done

# 固定 Ontop PostgresIdentifierTest 的四个 quoted/unquoted identifier 与 alias 结果。
for query in country country2 country3 country4; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-identifiers/baseline-identifiers.toml "tests/compat/postgres-identifiers/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-identifiers/$query.expected")
  assert_output "$actual" "$expected"
done
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-identifiers/baseline-lowercase.toml tests/compat/postgres-identifiers/country.rq)
expected=$(cat "$root/tests/compat/postgres-identifiers/lowercase-country.expected")
assert_output "$actual" "$expected"
for query in lower-birth-name lower-title; do
  actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-identifiers/baseline-lower-movie.toml "tests/compat/postgres-identifiers/$query.rq")
  expected=$(cat "$root/tests/compat/postgres-identifiers/$query.expected")
  assert_output "$actual" "$expected"
done
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-identifiers/baseline-prefix-source.toml tests/compat/postgres-identifiers/prefix-source.rq)
expected=$(cat "$root/tests/compat/postgres-identifiers/prefix-source.expected")
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-identifiers/baseline-npd.toml tests/compat/postgres-identifiers/quoted-alias.rq)
expected=$(cat "$root/tests/compat/postgres-identifiers/quoted-alias.expected")
assert_output "$actual" "$expected"

# 固定 CastPostgreSQLTest books 数据的 publication_date；放在其余 expression
# 场景后，避免改变它们各自取证的 books 输入。
docker exec "$name" psql -U rtop -d rtop_test -c "UPDATE books SET publication_date = CASE id WHEN 1 THEN TIMESTAMP '2014-06-05 16:47:52' WHEN 2 THEN TIMESTAMP '2011-12-08 11:30:00' WHEN 3 THEN TIMESTAMP '2015-09-21 09:23:06' WHEN 4 THEN TIMESTAMP '1970-11-05 07:50:00' END WHERE id IN (1, 2, 3, 4)" >/dev/null
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-cast/books.toml tests/compat/postgres-cast/cast-date-from-mapped-datetime.rq)
expected=$(cat "$root/tests/compat/postgres-cast/cast-date-from-mapped-datetime.expected")
assert_output "$actual" "$expected"

# 固定 Ontop AnnotationMovieTest 的原始 SPARQL。此阶段必须位于所有共享
# title/movie_info 场景之后：精确 seed 会清空这四张 IMDB 表。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-annotation/movie-exact-init.sql"
assert_annotation_movie_rows() {
  query=$1
  expected_rows=$2
  rows=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-annotation/movie.toml "$query" | wc -l | tr -d ' ')
  [ "$rows" = "$expected_rows" ]
}
assert_annotation_movie_rows tests/compat/postgres-annotation/exact-description.rq 444090
assert_annotation_movie_rows tests/compat/postgres-annotation/exact-date.rq 443300
assert_annotation_movie_rows tests/compat/postgres-annotation/exact-gross.rq 112576
assert_annotation_movie_rows tests/compat/postgres-annotation/exact-genre.rq 546032
assert_annotation_movie_rows tests/compat/postgres-annotation/exact-actor.rq 100000
assert_annotation_movie_rows tests/compat/postgres-annotation/exact-vip.rq 100000
assert_annotation_movie_rows tests/compat/postgres-annotation/exact-company-id.rq 131645
assert_annotation_movie_rows tests/compat/postgres-annotation/exact-id-title.rq 444090

# 固定 ImdbPostgresTest 的基础 mapping/query 计数；最后重建共享 IMDB 表。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-imdb/basic-init.sql"
assert_imdb_rows() {
  query=$1
  expected_rows=$2
  rows=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-imdb/imdb.toml "$query" | wc -l | tr -d ' ')
  [ "$rows" = "$expected_rows" ]
}
for item in \
  'find-actress 1' 'find-actor 1' 'find-writer 2' 'find-producer 1' 'find-director 1' 'find-editor 1' \
  'find-movie 1' 'find-tv-series 1' 'find-movie-genre 1' 'find-tv-series-genre 1' \
  'find-movie-budget 1' 'find-tv-series-budget 1' 'find-movie-gross 89' 'find-tv-series-gross 2' \
  'find-movie-actors 24' 'find-movie-year 1' 'find-tv-series-year 1' \
  'find-male-actors 18' 'find-actresses 6' 'find-directors 2' 'find-producers 3' 'find-editors 1'; do
  set -- $item
  assert_imdb_rows "tests/compat/postgres-imdb/$1.rq" "$2"
done
assert_imdb_rows tests/compat/postgres-imdb/birth-name-contains-z.rq 196531
assert_imdb_rows tests/compat/postgres-imdb/individuals.rq 29405
assert_imdb_rows tests/compat/postgres-imdb/company-location.rq 7738
assert_imdb_rows tests/compat/postgres-imdb/find-movie-company.rq 3
assert_imdb_rows tests/compat/postgres-imdb/western-europe-companies.rq 19167
assert_imdb_rows tests/compat/postgres-imdb/movie-from-asian-company.rq 15173
assert_imdb_rows tests/compat/postgres-imdb/east-asia-production-years.rq 8519
assert_imdb_rows tests/compat/postgres-imdb/east-asian-company-blank-node.rq 15173
assert_imdb_rows tests/compat/postgres-imdb/east-asia-action-movies.rq 2127
assert_imdb_rows tests/compat/postgres-imdb/east-asia-actor-directors.rq 36
assert_imdb_rows tests/compat/postgres-imdb/full-information.rq 12816
assert_imdb_rows tests/compat/postgres-imdb/q5.rq 0
assert_imdb_rows tests/compat/postgres-imdb/q4.rq 2136

# 固定 ImdbPostgresTest rating 排序场景；追加数据不影响此前基础精确计数。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-imdb/order-init.sql"
assert_imdb_rows tests/compat/postgres-imdb/top-action-ratings.rq 25
assert_imdb_rows tests/compat/postgres-imdb/bottom-ratings.rq 10

docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-imdb/q2-init.sql"
assert_imdb_rows tests/compat/postgres-imdb/q2.rq 57216

docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-imdb/q1-init.sql"
assert_imdb_rows tests/compat/postgres-imdb/q1.rq 18

docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-imdb/q3-init.sql"
assert_imdb_rows tests/compat/postgres-imdb/q3.rq 42

# 固定 UnboundVariableIMDbTest 的真实 simplify mapping 与 Series LIMIT 场景。
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-imdb/series-init.sql"
rows=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-imdb/series.toml tests/compat/postgres-imdb/series-limit.rq | wc -l | tr -d ' ')
[ "$rows" = "10" ]

# 固定 GeoSPARQLPostGISTest：PostGIS 独立镜像与端口避免同普通 PostgreSQL
# gate 争用 55432。entrypoint 会先启动一次临时数据库，故要求两次稳定探针。
postgis_name=rtop-postgis-compat-$$
postgis_cleanup() { docker rm -f "$postgis_name" >/dev/null 2>&1 || true; }
trap 'cleanup; postgis_cleanup' EXIT INT TERM
docker run -d --name "$postgis_name" -e POSTGRES_USER=rtop -e POSTGRES_PASSWORD=rtop -e POSTGRES_DB=rtop_test -p 55433:5432 docker.m.daocloud.io/postgis/postgis:17-3.5 >/dev/null
stable=0
while [ "$stable" -lt 2 ]; do
  if docker exec "$postgis_name" psql -U rtop -d rtop_test -c 'SELECT PostGIS_Full_Version()' >/dev/null 2>&1; then
    stable=$((stable + 1))
  else
    stable=0
  fi
  sleep 2
done
docker exec -i "$postgis_name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-geospatial/init.sql"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-geospatial/rtop-postgis.toml tests/compat/postgres-geospatial/intersects.rq)
rows=$(printf '%s\n' "$actual" | sed '/^$/d' | wc -l | tr -d ' ')
[ "$rows" = "36" ]
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-geospatial/rtop-postgis.toml tests/compat/postgres-geospatial/intersection-1.rq)
[ -z "$actual" ]
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-geospatial/rtop-postgis.toml tests/compat/postgres-geospatial/intersection-2.rq)
expected='?v="POLYGON((2 5,7 5,7 2,2 2,2 5))"^^<http://www.opengis.net/ont/geosparql#wktLiteral>'
assert_output "$actual" "$expected"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-geospatial/rtop-postgis.toml tests/compat/postgres-geospatial/intersection-3.rq)
[ -z "$actual" ]
