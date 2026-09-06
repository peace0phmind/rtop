#!/usr/bin/env sh
set -eu

# #106 的第一个真实双边 case。Ontop 与 rtop 使用不同进程和不同客户端路径：
# Ontop 在 Java 容器中执行 CLI，rtop 由宿主 Rust CLI 执行。规范化仅处理两者
# 的外部文本结果，绝不调用 rtop 的 parser、serializer 或预期生成逻辑。
#
# ONTOP_HOME 必须指向由固定提交构建的 CLI 目录，并在 jdbc/ 中包含 PostgreSQL
# JDBC jar。DIFFERENTIAL_ARTIFACT_DIR 可指定要保留的原始/规范化输出目录。
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
baseline="$root/../ontop"
commit=5ec07573b18513f33dfcd59ac45fe26a81f9cdbd
: "${ONTOP_HOME:?请设置 ONTOP_HOME 为固定 Ontop 基线构建出的 CLI 目录}"
[ -x "$ONTOP_HOME/ontop" ]
[ -d "$ONTOP_HOME/jdbc" ]
find "$ONTOP_HOME/jdbc" -type f -name 'postgresql-*.jar' | grep -q .
[ "$(git -C "$baseline" rev-parse HEAD)" = "$commit" ]
postgres_image=${POSTGRES_IMAGE:-postgres:17@sha256:5c855ad7b85e68e48a62f34662853f38b57c1c1d80f3a927ab58034fd6d31c5e}
postgis_image=${POSTGIS_IMAGE:-docker.m.daocloud.io/postgis/postgis:17-3.5@sha256:01a6a70e41e6c4467c8f55f6063555ed72db2d6662cd0d571040d42eadaeb6f6}
case_name=${DIFFERENTIAL_CASE:-d001}
variant=${DIFFERENTIAL_D001_VARIANT:-template}
v1_option=

# D002c/e/f/h 是规范中不同 SQL/标识符错误的组合验收原子。suite 仍逐项启动
# 独立 Ontop 与 rtop 进程，汇总文件仅作为矩阵可复核的单一入口。
if [ "$case_name" = d002cefh ]; then
  suite_artifacts=${DIFFERENTIAL_ARTIFACT_DIR:-"$(mktemp -d)/artifacts"}
  mkdir -p "$suite_artifacts"
  for suite_case in d002c d002e d002f d002h; do
    ONTOP_HOME="$ONTOP_HOME" DIFFERENTIAL_CASE="$suite_case" \
      DIFFERENTIAL_ARTIFACT_DIR="$suite_artifacts/$suite_case" "$0"
  done
  jq -e 'all(.status; . == "passed")' \
    "$suite_artifacts/d002c/provenance.json" \
    "$suite_artifacts/d002e/provenance.json" \
    "$suite_artifacts/d002f/provenance.json" \
    "$suite_artifacts/d002h/provenance.json" >/dev/null
  for stream in ontop rtop; do
    for suite_case in d002c d002e d002f d002h; do
      printf '%s\n' "### $suite_case"
      cat "$suite_artifacts/$suite_case/$stream.raw"
    done > "$suite_artifacts/$stream.raw"
  done
  jq -s '
    {format:"mapping-rejection-suite",
     cases: map({case_id, status, mapping_behavior}),
     all_rejected: all(.[]; .status == "passed")}
  ' "$suite_artifacts/d002c/provenance.json" \
    "$suite_artifacts/d002e/provenance.json" \
    "$suite_artifacts/d002f/provenance.json" \
    "$suite_artifacts/d002h/provenance.json" > "$suite_artifacts/normalized.json"
  first_provenance="$suite_artifacts/d002c/provenance.json"
  jq -n \
    --arg ontop_commit "$(jq -r .ontop_commit "$first_provenance")" \
    --arg postgres_digest "$(jq -r .postgres_image_digest "$first_provenance")" \
    --arg required_postgres_digest "$(jq -r .required_postgres_image_digest "$first_provenance")" \
    --arg fixture 'test/rdb2rdf-compliance/src/test/resources/D002/{create.sql,r2rmlc.ttl,r2rmle.ttl,r2rmlf.ttl,r2rmlh.ttl}' \
    --arg ontop_raw_sha256 "$(sha256sum "$suite_artifacts/ontop.raw" | awk '{print $1}')" \
    --arg rtop_raw_sha256 "$(sha256sum "$suite_artifacts/rtop.raw" | awk '{print $1}')" \
    --arg normalized_sha256 "$(sha256sum "$suite_artifacts/normalized.json" | awk '{print $1}')" \
    '{schema_version:1, case_id:"differential-r2rml-d002-postgres-semantic-rejection",
      mapping_behavior:"reject undefined SQL identifiers, unquoted identifiers, and duplicate aliases",
      status:"passed", status_reason:"四个固定 PostgreSQL 17 fixture 均由双方独立 CLI 拒绝",
      ontop_commit:$ontop_commit, postgres_image_digest:$postgres_digest,
      required_postgres_image_digest:$required_postgres_digest, fixture:$fixture,
      query:"materialize dataset as N-Quads (rejection suite)", postgres_extensions:"none",
      independent_processes:{ontop:"Java 17 container CLI",rtop:"Rust host CLI"},
      sha256:{ontop_raw:$ontop_raw_sha256,rtop_raw:$rtop_raw_sha256,normalized:$normalized_sha256}}' \
    > "$suite_artifacts/provenance.json"
  printf '%s\n' "differential d002cefh/suite: passed; artifacts=$suite_artifacts"
  exit 0
fi

# 共享父类 OntopMappingOntologyRelatedCommand 的输入契约不是单个转换命令可代表
# 的：pretty、v1-to-v3、to-obda、to-r2rml 都必须各自经 PostgreSQL 重载验收。
if [ "$case_name" = clisharedinput ]; then
  suite_artifacts=${DIFFERENTIAL_ARTIFACT_DIR:-"$(mktemp -d)/artifacts"}
  mkdir -p "$suite_artifacts"
  for suite_case in clipretty cliv1native clitoobda clitor2rml; do
    ONTOP_HOME="$ONTOP_HOME" DIFFERENTIAL_CASE="$suite_case" \
      DIFFERENTIAL_ARTIFACT_DIR="$suite_artifacts/$suite_case" "$0"
  done
  jq -e 'all(.status; . == "passed")' \
    "$suite_artifacts/clipretty/provenance.json" \
    "$suite_artifacts/cliv1native/provenance.json" \
    "$suite_artifacts/clitoobda/provenance.json" \
    "$suite_artifacts/clitor2rml/provenance.json" >/dev/null
  for stream in ontop rtop; do
    for suite_case in clipretty cliv1native clitoobda clitor2rml; do
      printf '%s\n' "### $suite_case"
      cat "$suite_artifacts/$suite_case/$stream.raw"
    done > "$suite_artifacts/$stream.raw"
  done
  jq -s '{format:"cli-mapping-shared-input-contract",cases:map({case_id,status,mapping_behavior}),all_passed:all(.[];.status == "passed")}' \
    "$suite_artifacts/clipretty/provenance.json" \
    "$suite_artifacts/cliv1native/provenance.json" \
    "$suite_artifacts/clitoobda/provenance.json" \
    "$suite_artifacts/clitor2rml/provenance.json" > "$suite_artifacts/normalized.json"
  first_provenance="$suite_artifacts/clipretty/provenance.json"
  jq -n \
    --arg ontop_commit "$(jq -r .ontop_commit "$first_provenance")" \
    --arg postgres_digest "$(jq -r .postgres_image_digest "$first_provenance")" \
    --arg required_postgres_digest "$(jq -r .required_postgres_image_digest "$first_provenance")" \
    --arg ontop_raw_sha256 "$(sha256sum "$suite_artifacts/ontop.raw" | awk '{print $1}')" \
    --arg rtop_raw_sha256 "$(sha256sum "$suite_artifacts/rtop.raw" | awk '{print $1}')" \
    --arg normalized_sha256 "$(sha256sum "$suite_artifacts/normalized.json" | awk '{print $1}')" \
    '{schema_version:1,case_id:"differential-cli-mapping-shared-input-contract",mapping_behavior:"pretty-r2rml, v1-to-v3, to-obda, and to-r2rml share native/R2RML mapping plus ontology/facts input contract",status:"passed",status_reason:"四个固定 PostgreSQL 17 子命令均由双方独立 CLI 执行并完成 reload 或 rejection 验收",ontop_commit:$ontop_commit,postgres_image_digest:$postgres_digest,required_postgres_image_digest:$required_postgres_digest,fixture:"fixed CLI mapping conversion fixtures with PostgreSQL reload",query:"pretty, v1-to-v3, to-obda, to-r2rml",postgres_extensions:"none",independent_processes:{ontop:"Java 17 container CLI",rtop:"Rust host CLI"},sha256:{ontop_raw:$ontop_raw_sha256,rtop_raw:$rtop_raw_sha256,normalized:$normalized_sha256}}' \
    > "$suite_artifacts/provenance.json"
  printf '%s\n' "differential clisharedinput/suite: passed; artifacts=$suite_artifacts"
  exit 0
fi

# Direct Mapping HTTP 需要先由各自的进程构造 mapping：Ontop endpoint 不支持
# 直接传入 catalog，而 rtop endpoint 使用 native direct_mapping 配置。两个子例
# 分别覆盖 D009 的 PK/FK/NULL 和 D010 的 percent-encoding。
if [ "$case_name" = dmdhttp ]; then
  suite_artifacts=${DIFFERENTIAL_ARTIFACT_DIR:-"$(mktemp -d)/artifacts"}
  mkdir -p "$suite_artifacts"
  for suite_case in dmd009http dmd010http; do
    ONTOP_HOME="$ONTOP_HOME" DIFFERENTIAL_CASE="$suite_case" \
      DIFFERENTIAL_ARTIFACT_DIR="$suite_artifacts/$suite_case" "$0"
  done
  jq -e 'all(.status; . == "passed")' \
    "$suite_artifacts/dmd009http/provenance.json" \
    "$suite_artifacts/dmd010http/provenance.json" >/dev/null
  for stream in ontop rtop; do
    for suite_case in dmd009http dmd010http; do
      printf '%s\n' "### $suite_case"
      cat "$suite_artifacts/$suite_case/$stream.raw"
    done > "$suite_artifacts/$stream.raw"
  done
  jq -s '{format:"direct-mapping-http-suite",cases:map({case_id,status,mapping_behavior}),all_passed:all(.[];.status == "passed")}' \
    "$suite_artifacts/dmd009http/provenance.json" \
    "$suite_artifacts/dmd010http/provenance.json" > "$suite_artifacts/normalized.json"
  first_provenance="$suite_artifacts/dmd009http/provenance.json"
  jq -n \
    --arg ontop_commit "$(jq -r .ontop_commit "$first_provenance")" \
    --arg postgres_digest "$(jq -r .postgres_image_digest "$first_provenance")" \
    --arg required_postgres_digest "$(jq -r .required_postgres_image_digest "$first_provenance")" \
    --arg fixture 'test/rdb2rdf-compliance/src/test/resources/D009,D010/{create.sql,directGraph.ttl}; Ontop bootstrap + endpoint; rtop direct_mapping endpoint' \
    --arg ontop_raw_sha256 "$(sha256sum "$suite_artifacts/ontop.raw" | awk '{print $1}')" \
    --arg rtop_raw_sha256 "$(sha256sum "$suite_artifacts/rtop.raw" | awk '{print $1}')" \
    --arg normalized_sha256 "$(sha256sum "$suite_artifacts/normalized.json" | awk '{print $1}')" \
    '{schema_version:1,case_id:"differential-postgres-direct-mapping-pk-fk-null-encoding-endpoint",mapping_behavior:"Direct Mapping endpoint preserves D009 PK/FK/NULL and D010 percent-encoding",status:"passed",status_reason:"固定 PostgreSQL 17 digest 下的双边 HTTP JSON 结果一致",ontop_commit:$ontop_commit,postgres_image_digest:$postgres_digest,required_postgres_image_digest:$required_postgres_digest,fixture:$fixture,query:"D009 FK SELECT; D010 percent-encoded IRI SELECT",postgres_extensions:"none",independent_processes:{ontop:"Java 17 container endpoint",rtop:"Rust host endpoint"},sha256:{ontop_raw:$ontop_raw_sha256,rtop_raw:$rtop_raw_sha256,normalized:$normalized_sha256}}' \
    > "$suite_artifacts/provenance.json"
  printf '%s\n' "differential dmdhttp/suite: passed; artifacts=$suite_artifacts"
  exit 0
fi

# #58 必须同时覆盖逐行 decimal/ROUND 与 aggregate 前后 ROUND 的不同语义。
if [ "$case_name" = httpdecimal ]; then
  suite_artifacts=${DIFFERENTIAL_ARTIFACT_DIR:-"$(mktemp -d)/artifacts"}
  mkdir -p "$suite_artifacts"
  for suite_case in httpdecimalscalar httpdecimalaggregate; do
    ONTOP_HOME="$ONTOP_HOME" DIFFERENTIAL_CASE="$suite_case" \
      DIFFERENTIAL_ARTIFACT_DIR="$suite_artifacts/$suite_case" "$0"
  done
  jq -e 'all(.status; . == "passed")' \
    "$suite_artifacts/httpdecimalscalar/provenance.json" \
    "$suite_artifacts/httpdecimalaggregate/provenance.json" >/dev/null
  for stream in ontop rtop; do
    for suite_case in httpdecimalscalar httpdecimalaggregate; do
      printf '%s\n' "### $suite_case"
      cat "$suite_artifacts/$suite_case/$stream.raw"
    done > "$suite_artifacts/$stream.raw"
  done
  jq -s '{format:"decimal-round-http-suite",cases:map({case_id,status,mapping_behavior}),all_passed:all(.[];.status == "passed")}' \
    "$suite_artifacts/httpdecimalscalar/provenance.json" \
    "$suite_artifacts/httpdecimalaggregate/provenance.json" > "$suite_artifacts/normalized.json"
  first_provenance="$suite_artifacts/httpdecimalscalar/provenance.json"
  jq -n \
    --arg ontop_commit "$(jq -r .ontop_commit "$first_provenance")" \
    --arg postgres_digest "$(jq -r .postgres_image_digest "$first_provenance")" \
    --arg required_postgres_digest "$(jq -r .required_postgres_image_digest "$first_provenance")" \
    --arg ontop_raw_sha256 "$(sha256sum "$suite_artifacts/ontop.raw" | awk '{print $1}')" \
    --arg rtop_raw_sha256 "$(sha256sum "$suite_artifacts/rtop.raw" | awk '{print $1}')" \
    --arg normalized_sha256 "$(sha256sum "$suite_artifacts/normalized.json" | awk '{print $1}')" \
    '{schema_version:1,case_id:"differential-postgres-decimal-round-exact-endpoint",mapping_behavior:"PostgreSQL decimal arithmetic, scalar ROUND, and aggregate-before/after ROUND",status:"passed",status_reason:"固定 PostgreSQL 17 digest 下的双边 HTTP JSON 结果一致",ontop_commit:$ontop_commit,postgres_image_digest:$postgres_digest,required_postgres_image_digest:$required_postgres_digest,fixture:"tests/compat/postgres-http/{algebra-init.sql,mapping-algebra-bag.obda,decimal-round.rq,decimal-round-aggregate.rq}",query:"scalar and aggregate decimal ROUND SELECT",postgres_extensions:"none",independent_processes:{ontop:"Java 17 container endpoint",rtop:"Rust host endpoint"},sha256:{ontop_raw:$ontop_raw_sha256,rtop_raw:$rtop_raw_sha256,normalized:$normalized_sha256}}' \
    > "$suite_artifacts/provenance.json"
  printf '%s\n' "differential httpdecimal/suite: passed; artifacts=$suite_artifacts"
  exit 0
fi

# #61 只有同时覆盖 FROM、FROM NAMED/GRAPH 和 SERVICE 的固定未支持边界，
# 才能代表 dataset/graph 行为；任何单一子场景都不能关闭该项。
if [ "$case_name" = httpdataset ]; then
  suite_artifacts=${DIFFERENTIAL_ARTIFACT_DIR:-"$(mktemp -d)/artifacts"}
  mkdir -p "$suite_artifacts"
  for suite_case in httpdatasetdefault httpdatasetnamed httpservice; do
    ONTOP_HOME="$ONTOP_HOME" DIFFERENTIAL_CASE="$suite_case" \
      DIFFERENTIAL_ARTIFACT_DIR="$suite_artifacts/$suite_case" "$0"
  done
  jq -e 'all(.status; . == "passed")' \
    "$suite_artifacts/httpdatasetdefault/provenance.json" \
    "$suite_artifacts/httpdatasetnamed/provenance.json" \
    "$suite_artifacts/httpservice/provenance.json" >/dev/null
  for stream in ontop rtop; do
    for suite_case in httpdatasetdefault httpdatasetnamed httpservice; do
      printf '%s\n' "### $suite_case"
      cat "$suite_artifacts/$suite_case/$stream.raw"
    done > "$suite_artifacts/$stream.raw"
  done
  jq -s '{format:"dataset-graph-service-http-suite",cases:map({case_id,status,mapping_behavior}),all_passed:all(.[];.status == "passed")}' \
    "$suite_artifacts/httpdatasetdefault/provenance.json" \
    "$suite_artifacts/httpdatasetnamed/provenance.json" \
    "$suite_artifacts/httpservice/provenance.json" > "$suite_artifacts/normalized.json"
  first_provenance="$suite_artifacts/httpdatasetdefault/provenance.json"
  jq -n \
    --arg ontop_commit "$(jq -r .ontop_commit "$first_provenance")" \
    --arg postgres_digest "$(jq -r .postgres_image_digest "$first_provenance")" \
    --arg required_postgres_digest "$(jq -r .required_postgres_image_digest "$first_provenance")" \
    --arg ontop_raw_sha256 "$(sha256sum "$suite_artifacts/ontop.raw" | awk '{print $1}')" \
    --arg rtop_raw_sha256 "$(sha256sum "$suite_artifacts/rtop.raw" | awk '{print $1}')" \
    --arg normalized_sha256 "$(sha256sum "$suite_artifacts/normalized.json" | awk '{print $1}')" \
    '{schema_version:1,case_id:"differential-postgres-dataset-graph-service-boundary-endpoint",mapping_behavior:"PostgreSQL FROM/FROM NAMED named graph visibility and unsupported SERVICE boundary",status:"passed",status_reason:"固定 PostgreSQL 17 digest 下的双边 HTTP JSON 结果及 SERVICE HTTP 500 未支持边界一致",ontop_commit:$ontop_commit,postgres_image_digest:$postgres_digest,required_postgres_image_digest:$required_postgres_digest,fixture:"tests/compat/postgres-http/{algebra-init.sql,mapping-algebra-bag.obda,dataset-graph.rq,dataset-named-graph.rq}; inline SERVICE query",query:"FROM, FROM NAMED/GRAPH, and SERVICE SELECT",postgres_extensions:"none",independent_processes:{ontop:"Java 17 container endpoint",rtop:"Rust host endpoint"},sha256:{ontop_raw:$ontop_raw_sha256,rtop_raw:$rtop_raw_sha256,normalized:$normalized_sha256}}' \
    > "$suite_artifacts/provenance.json"
  printf '%s\n' "differential httpdataset/suite: passed; artifacts=$suite_artifacts"
  exit 0
fi

# #44 的 PostgreSQL source SQL 正则语义必须同时覆盖正向 `~*` 与否定 `!~*`；
# 两个映射的结果不能以单一计数断言互相替代。
if [ "$case_name" = httpregex ]; then
  suite_artifacts=${DIFFERENTIAL_ARTIFACT_DIR:-"$(mktemp -d)/artifacts"}
  mkdir -p "$suite_artifacts"
  for suite_case in httpregexaddress httpregexperson; do
    ONTOP_HOME="$ONTOP_HOME" DIFFERENTIAL_CASE="$suite_case" \
      DIFFERENTIAL_ARTIFACT_DIR="$suite_artifacts/$suite_case" "$0"
  done
  jq -e 'all(.status; . == "passed")' \
    "$suite_artifacts/httpregexaddress/provenance.json" \
    "$suite_artifacts/httpregexperson/provenance.json" >/dev/null
  for stream in ontop rtop; do
    for suite_case in httpregexaddress httpregexperson; do
      printf '%s\n' "### $suite_case"
      cat "$suite_artifacts/$suite_case/$stream.raw"
    done > "$suite_artifacts/$stream.raw"
  done
  jq -s '{format:"postgres-source-regex-http-suite",cases:map({case_id,status,mapping_behavior}),all_passed:all(.[];.status == "passed")}' \
    "$suite_artifacts/httpregexaddress/provenance.json" \
    "$suite_artifacts/httpregexperson/provenance.json" > "$suite_artifacts/normalized.json"
  first_provenance="$suite_artifacts/httpregexaddress/provenance.json"
  jq -n \
    --arg ontop_commit "$(jq -r .ontop_commit "$first_provenance")" \
    --arg postgres_digest "$(jq -r .postgres_image_digest "$first_provenance")" \
    --arg required_postgres_digest "$(jq -r .required_postgres_image_digest "$first_provenance")" \
    --arg ontop_raw_sha256 "$(sha256sum "$suite_artifacts/ontop.raw" | awk '{print $1}')" \
    --arg rtop_raw_sha256 "$(sha256sum "$suite_artifacts/rtop.raw" | awk '{print $1}')" \
    --arg normalized_sha256 "$(sha256sum "$suite_artifacts/normalized.json" | awk '{print $1}')" \
    '{schema_version:1,case_id:"differential-postgres-source-sql-regex-negation-endpoint",mapping_behavior:"PostgreSQL source SQL case-insensitive regex and negated regex mappings",status:"passed",status_reason:"固定 PostgreSQL 17 digest 下的双边 HTTP JSON RDF bindings 一致",ontop_commit:$ontop_commit,postgres_image_digest:$postgres_digest,required_postgres_image_digest:$required_postgres_digest,fixture:"test/docker-tests/src/test/resources/pgsql/regex/stockexchangeRegex.obda; tests/compat/postgres-expressions/{init.sql,regex-address.rq,regex-person.rq}",query:"BolzanoAddress and PhysicalPerson SELECT",postgres_extensions:"none",independent_processes:{ontop:"Java 17 container endpoint",rtop:"Rust host endpoint"},sha256:{ontop_raw:$ontop_raw_sha256,rtop_raw:$rtop_raw_sha256,normalized:$normalized_sha256}}' \
    > "$suite_artifacts/provenance.json"
  printf '%s\n' "differential httpregex/suite: passed; artifacts=$suite_artifacts"
  exit 0
fi

# #44 的 cast 原子同时要求 PostgreSQL lexical override 与非法 cast 的 expression
# error；任何一个子查询单独通过都不足以代表该边界。
if [ "$case_name" = httpcast ]; then
  suite_artifacts=${DIFFERENTIAL_ARTIFACT_DIR:-"$(mktemp -d)/artifacts"}
  mkdir -p "$suite_artifacts"
  for suite_case in httpcastscalar httpcastinvalid; do
    ONTOP_HOME="$ONTOP_HOME" DIFFERENTIAL_CASE="$suite_case" \
      DIFFERENTIAL_ARTIFACT_DIR="$suite_artifacts/$suite_case" "$0"
  done
  jq -e 'all(.status; . == "passed")' \
    "$suite_artifacts/httpcastscalar/provenance.json" \
    "$suite_artifacts/httpcastinvalid/provenance.json" >/dev/null
  for stream in ontop rtop; do
    for suite_case in httpcastscalar httpcastinvalid; do
      printf '%s\n' "### $suite_case"
      cat "$suite_artifacts/$suite_case/$stream.raw"
    done > "$suite_artifacts/$stream.raw"
  done
  jq -s '{format:"postgres-cast-http-suite",cases:map({case_id,status,mapping_behavior}),all_passed:all(.[];.status == "passed")}' \
    "$suite_artifacts/httpcastscalar/provenance.json" \
    "$suite_artifacts/httpcastinvalid/provenance.json" > "$suite_artifacts/normalized.json"
  first_provenance="$suite_artifacts/httpcastscalar/provenance.json"
  jq -n \
    --arg ontop_commit "$(jq -r .ontop_commit "$first_provenance")" \
    --arg postgres_digest "$(jq -r .postgres_image_digest "$first_provenance")" \
    --arg required_postgres_digest "$(jq -r .required_postgres_image_digest "$first_provenance")" \
    --arg ontop_raw_sha256 "$(sha256sum "$suite_artifacts/ontop.raw" | awk '{print $1}')" \
    --arg rtop_raw_sha256 "$(sha256sum "$suite_artifacts/rtop.raw" | awk '{print $1}')" \
    --arg normalized_sha256 "$(sha256sum "$suite_artifacts/normalized.json" | awk '{print $1}')" \
    '{schema_version:1,case_id:"differential-postgres-cast-lexical-and-invalid-input-endpoint",mapping_behavior:"PostgreSQL CastPostgreSQL float lexical override and invalid cast expression errors",status:"passed",status_reason:"固定 PostgreSQL 17 digest 下的双边 HTTP JSON RDF bindings 一致",ontop_commit:$ontop_commit,postgres_image_digest:$postgres_digest,required_postgres_digest:$required_postgres_digest,fixture:"test/lightweight-tests/src/test/resources/books/books.obda; tests/compat/postgres-expressions/init.sql; tests/compat/postgres-cast cast queries",query:"xsd:float from xsd:double and invalid numeric casts",postgres_extensions:"none",independent_processes:{ontop:"Java 17 container endpoint",rtop:"Rust host endpoint"},sha256:{ontop_raw:$ontop_raw_sha256,rtop_raw:$rtop_raw_sha256,normalized:$normalized_sha256}}' \
    > "$suite_artifacts/provenance.json"
  printf '%s\n' "differential httpcast/suite: passed; artifacts=$suite_artifacts"
  exit 0
fi

# University property closure 同时要求 subPropertyOf 与 inverseOf 两个方向的
# endpoint 结果；任一单向查询都不足以代表该 PostgreSQL mapping 原子。
if [ "$case_name" = httpuniversityproperty ]; then
  suite_artifacts=${DIFFERENTIAL_ARTIFACT_DIR:-"$(mktemp -d)/artifacts"}
  mkdir -p "$suite_artifacts"
  for suite_case in httpuniversitysubproperty httpuniversityinverse; do
    ONTOP_HOME="$ONTOP_HOME" DIFFERENTIAL_CASE="$suite_case" \
      DIFFERENTIAL_ARTIFACT_DIR="$suite_artifacts/$suite_case" "$0"
  done
  jq -e 'all(.status; . == "passed")' \
    "$suite_artifacts/httpuniversitysubproperty/provenance.json" \
    "$suite_artifacts/httpuniversityinverse/provenance.json" >/dev/null
  for stream in ontop rtop; do
    for suite_case in httpuniversitysubproperty httpuniversityinverse; do
      printf '%s\n' "### $suite_case"
      cat "$suite_artifacts/$suite_case/$stream.raw"
    done > "$suite_artifacts/$stream.raw"
  done
  jq -s '{format:"university-property-closure-http-suite",cases:map({case_id,status,mapping_behavior}),all_passed:all(.[];.status == "passed")}' \
    "$suite_artifacts/httpuniversitysubproperty/provenance.json" \
    "$suite_artifacts/httpuniversityinverse/provenance.json" > "$suite_artifacts/normalized.json"
  first_provenance="$suite_artifacts/httpuniversitysubproperty/provenance.json"
  jq -n \
    --arg ontop_commit "$(jq -r .ontop_commit "$first_provenance")" \
    --arg postgres_digest "$(jq -r .postgres_image_digest "$first_provenance")" \
    --arg required_postgres_digest "$(jq -r .required_postgres_image_digest "$first_provenance")" \
    --arg ontop_raw_sha256 "$(sha256sum "$suite_artifacts/ontop.raw" | awk '{print $1}')" \
    --arg rtop_raw_sha256 "$(sha256sum "$suite_artifacts/rtop.raw" | awk '{print $1}')" \
    --arg normalized_sha256 "$(sha256sum "$suite_artifacts/normalized.json" | awk '{print $1}')" \
    '{schema_version:1,case_id:"differential-postgres-ontology-subproperty-inverse-rewrite-endpoint",mapping_behavior:"PostgreSQL givesLab subproperty and isTaughtBy inverse property closure",status:"passed",status_reason:"固定 PostgreSQL 17 digest 下的两项双边 HTTP JSON 结果一致",ontop_commit:$ontop_commit,postgres_image_digest:$postgres_digest,required_postgres_image_digest:$required_postgres_digest,fixture:"binding/rdf4j/src/test/resources/tbox-facts/{university.sql,university-complete.ttl}; tests/compat/postgres-ontology/{endpoint-imports-university.ttl,teaching.obda,subproperty.rq,inverse.rq}",query:"subPropertyOf and inverseOf SELECT",postgres_extensions:"none",independent_processes:{ontop:"Java 17 container endpoint",rtop:"Rust host endpoint"},sha256:{ontop_raw:$ontop_raw_sha256,rtop_raw:$rtop_raw_sha256,normalized:$normalized_sha256}}' \
    > "$suite_artifacts/provenance.json"
  printf '%s\n' "differential httpuniversityproperty/suite: passed; artifacts=$suite_artifacts"
  exit 0
fi

# JSON、JSONB 与二维 array 的 aggregate lexical 结果各自经过不同的 PostgreSQL
# source SQL；三者都通过才代表 NestedData PostgreSQL 原子。
if [ "$case_name" = httpnestedaggregate ]; then
  suite_artifacts=${DIFFERENTIAL_ARTIFACT_DIR:-"$(mktemp -d)/artifacts"}
  mkdir -p "$suite_artifacts"
  for suite_case in httpnestedjson httpnestedjsonb httpnestedarray; do
    ONTOP_HOME="$ONTOP_HOME" DIFFERENTIAL_CASE="$suite_case" \
      DIFFERENTIAL_ARTIFACT_DIR="$suite_artifacts/$suite_case" "$0"
  done
  jq -e 'all(.status; . == "passed")' \
    "$suite_artifacts/httpnestedjson/provenance.json" \
    "$suite_artifacts/httpnestedjsonb/provenance.json" \
    "$suite_artifacts/httpnestedarray/provenance.json" >/dev/null
  for stream in ontop rtop; do
    for suite_case in httpnestedjson httpnestedjsonb httpnestedarray; do
      printf '%s\n' "### $suite_case"
      cat "$suite_artifacts/$suite_case/$stream.raw"
    done > "$suite_artifacts/$stream.raw"
  done
  jq -s '{format:"postgres-nested-aggregate-http-suite",cases:map({case_id,status,mapping_behavior}),all_passed:all(.[];.status == "passed")}' \
    "$suite_artifacts/httpnestedjson/provenance.json" \
    "$suite_artifacts/httpnestedjsonb/provenance.json" \
    "$suite_artifacts/httpnestedarray/provenance.json" > "$suite_artifacts/normalized.json"
  first_provenance="$suite_artifacts/httpnestedjson/provenance.json"
  jq -n \
    --arg ontop_commit "$(jq -r .ontop_commit "$first_provenance")" \
    --arg postgres_digest "$(jq -r .postgres_image_digest "$first_provenance")" \
    --arg required_postgres_digest "$(jq -r .required_postgres_image_digest "$first_provenance")" \
    --arg ontop_raw_sha256 "$(sha256sum "$suite_artifacts/ontop.raw" | awk '{print $1}')" \
    --arg rtop_raw_sha256 "$(sha256sum "$suite_artifacts/rtop.raw" | awk '{print $1}')" \
    --arg normalized_sha256 "$(sha256sum "$suite_artifacts/normalized.json" | awk '{print $1}')" \
    '{schema_version:1,case_id:"differential-postgres-nested-json-jsonb-array-aggregate-endpoint",mapping_behavior:"PostgreSQL JSON, JSONB, and array LATERAL/unnest aggregate lexical bindings",status:"passed",status_reason:"固定 PostgreSQL 17 digest 下三种 source SQL 的双边 HTTP JSON 结果一致",ontop_commit:$ontop_commit,postgres_image_digest:$postgres_digest,required_postgres_image_digest:$required_postgres_digest,fixture:"tests/compat/postgres-nested/{init.sql,nested-json.obda,nested-jsonb.obda,nested-array.obda,flatten-aggregate.rq}",query:"JSON/JSONB/array AVG aggregate SELECT",postgres_extensions:"none",independent_processes:{ontop:"Java 17 container endpoint",rtop:"Rust host endpoint"},sha256:{ontop_raw:$ontop_raw_sha256,rtop_raw:$rtop_raw_sha256,normalized:$normalized_sha256}}' \
    > "$suite_artifacts/provenance.json"
  printf '%s\n' "differential httpnestedaggregate/suite: passed; artifacts=$suite_artifacts"
  exit 0
fi

# GeoSPARQLPostGISTest 的成功 intersection 与两类 geometry/geography 混合
# 反事实必须在真实 PostGIS 上共同成立；postgres:17 的普通镜像不能替代它。
if [ "$case_name" = httppostgisintersection ]; then
  suite_artifacts=${DIFFERENTIAL_ARTIFACT_DIR:-"$(mktemp -d)/artifacts"}
  mkdir -p "$suite_artifacts"
  for suite_case in httppostgisintersection1 httppostgisintersection2 httppostgisintersection3; do
    ONTOP_HOME="$ONTOP_HOME" POSTGRES_IMAGE="$postgis_image" POSTGIS_IMAGE="$postgis_image" \
      DIFFERENTIAL_CASE="$suite_case" DIFFERENTIAL_ARTIFACT_DIR="$suite_artifacts/$suite_case" "$0"
  done
  jq -e 'all(.status; . == "passed")' \
    "$suite_artifacts/httppostgisintersection1/provenance.json" \
    "$suite_artifacts/httppostgisintersection2/provenance.json" \
    "$suite_artifacts/httppostgisintersection3/provenance.json" >/dev/null
  for stream in ontop rtop; do
    for suite_case in httppostgisintersection1 httppostgisintersection2 httppostgisintersection3; do
      printf '%s\n' "### $suite_case"
      cat "$suite_artifacts/$suite_case/$stream.raw"
    done > "$suite_artifacts/$stream.raw"
  done
  jq -s '{format:"postgres-postgis-intersection-http-suite",cases:map({case_id,status,mapping_behavior}),all_passed:all(.[];.status == "passed")}' \
    "$suite_artifacts/httppostgisintersection1/provenance.json" \
    "$suite_artifacts/httppostgisintersection2/provenance.json" \
    "$suite_artifacts/httppostgisintersection3/provenance.json" > "$suite_artifacts/normalized.json"
  first_provenance="$suite_artifacts/httppostgisintersection1/provenance.json"
  jq -n \
    --arg ontop_commit "$(jq -r .ontop_commit "$first_provenance")" \
    --arg postgres_digest "$(jq -r .postgres_image_digest "$first_provenance")" \
    --arg required_postgres_digest "$(jq -r .required_postgres_image_digest "$first_provenance")" \
    --arg ontop_raw_sha256 "$(sha256sum "$suite_artifacts/ontop.raw" | awk '{print $1}')" \
    --arg rtop_raw_sha256 "$(sha256sum "$suite_artifacts/rtop.raw" | awk '{print $1}')" \
    --arg normalized_sha256 "$(sha256sum "$suite_artifacts/normalized.json" | awk '{print $1}')" \
    '{schema_version:1,case_id:"differential-postgres-postgis-geometry-geography-intersection-endpoint",mapping_behavior:"GeoSPARQL PostGIS geometry/geography buffer intersection and mixed-type empty results",status:"passed",status_reason:"固定 PostGIS 17/3.5 digest 下三项双边 HTTP JSON 结果一致",ontop_commit:$ontop_commit,postgres_image_digest:$postgres_digest,required_postgres_image_digest:$required_postgres_digest,fixture:"test/lightweight-tests/src/test/resources/geospatial/geospatial.obda; tests/compat/postgres-geospatial/{init.sql,geospatial.obda,intersection-1.rq,intersection-2.rq,intersection-3.rq}",query:"geometry/geography buffer intersection plus two mixed-type counterfactuals",postgres_extensions:"postgis",independent_processes:{ontop:"Java 17 container endpoint",rtop:"Rust host endpoint"},sha256:{ontop_raw:$ontop_raw_sha256,rtop_raw:$rtop_raw_sha256,normalized:$normalized_sha256}}' \
    > "$suite_artifacts/provenance.json"
  printf '%s\n' "differential httppostgisintersection/suite: passed; artifacts=$suite_artifacts"
  exit 0
fi

# FactsFile 的 I/O 与语法错误在 Ontop endpoint 初始化阶段拒绝。两个子例均要求
# 两端 listener 未就绪，并分别把 JVM FactsException 与 Rust invalid-facts 归入
# 同一稳定外部类别，而不比较语言相关的错误文本。
if [ "$case_name" = httpfactserrors ]; then
  suite_artifacts=${DIFFERENTIAL_ARTIFACT_DIR:-"$(mktemp -d)/artifacts"}
  mkdir -p "$suite_artifacts"
  for suite_case in httpfactsmissing httpfactsmalformed; do
    if ONTOP_HOME="$ONTOP_HOME" DIFFERENTIAL_CASE="$suite_case" \
      DIFFERENTIAL_ARTIFACT_DIR="$suite_artifacts/$suite_case" "$0"; then
      printf '%s\n' "facts rejection unexpectedly started: $suite_case" >&2
      exit 1
    fi
    grep -q 'FactsException' "$suite_artifacts/$suite_case/ontop.endpoint.log"
    grep -q 'invalid-facts:' "$suite_artifacts/$suite_case/rtop.endpoint.log"
  done
  for stream in ontop rtop; do
    for suite_case in httpfactsmissing httpfactsmalformed; do
      printf '%s\n' "### $suite_case"
      cat "$suite_artifacts/$suite_case/$stream.endpoint.log"
    done > "$suite_artifacts/$stream.raw"
  done
  jq -n '{format:"facts-endpoint-startup-rejection", cases:["missing","malformed"], ontop_category:"FactsException", rtop_category:"invalid-facts", both_rejected_before_health:true}' > "$suite_artifacts/normalized.json"
  jq -n \
    --arg ontop_commit "$commit" \
    --arg postgres_digest "$(docker image inspect "$postgres_image" --format '{{range .RepoDigests}}{{println .}}{{end}}' | awk -F@ '/@sha256:/ { print $2; exit }')" \
    --arg required_postgres_digest sha256:5c855ad7b85e68e48a62f34662853f38b57c1c1d80f3a927ab58034fd6d31c5e \
    --arg ontop_raw_sha256 "$(sha256sum "$suite_artifacts/ontop.raw" | awk '{print $1}')" \
    --arg rtop_raw_sha256 "$(sha256sum "$suite_artifacts/rtop.raw" | awk '{print $1}')" \
    --arg normalized_sha256 "$(sha256sum "$suite_artifacts/normalized.json" | awk '{print $1}')" \
    '{schema_version:1,case_id:"differential-postgres-facts-missing-malformed-error-endpoint",mapping_behavior:"missing and malformed Turtle facts reject both endpoints before health readiness",status:"passed",status_reason:"固定 PostgreSQL 17 digest 下两端均以各自稳定 facts 错误类别拒绝启动",ontop_commit:$ontop_commit,postgres_image_digest:$postgres_digest,required_postgres_image_digest:$required_postgres_digest,fixture:"binding/rdf4j/src/test/resources/facts/mapping.obda; tests/compat/postgres-facts/{missing.ttl,malformed.ttl}",query:"endpoint startup rejection",postgres_extensions:"none",independent_processes:{ontop:"Java 17 container endpoint",rtop:"Rust host endpoint"},sha256:{ontop_raw:$ontop_raw_sha256,rtop_raw:$rtop_raw_sha256,normalized:$normalized_sha256}}' \
    > "$suite_artifacts/provenance.json"
  printf '%s\n' "differential httpfactserrors/suite: passed; artifacts=$suite_artifacts"
  exit 0
fi

if [ "$case_name" = httppath ]; then
  suite_artifacts=${DIFFERENTIAL_ARTIFACT_DIR:-"$(mktemp -d)/artifacts"}
  mkdir -p "$suite_artifacts"
  for suite_case in httppathexists httppathnotexists; do
    ONTOP_HOME="$ONTOP_HOME" DIFFERENTIAL_CASE="$suite_case" DIFFERENTIAL_ARTIFACT_DIR="$suite_artifacts/$suite_case" "$0"
  done
  jq -e 'all(.status; . == "passed")' "$suite_artifacts/httppathexists/provenance.json" "$suite_artifacts/httppathnotexists/provenance.json" >/dev/null
  for stream in ontop rtop; do
    for suite_case in httppathexists httppathnotexists; do
      printf '%s\n' "### $suite_case"
      cat "$suite_artifacts/$suite_case/$stream.raw"
    done > "$suite_artifacts/$stream.raw"
  done
  jq -s '{format:"property-path-exists-http-suite",cases:map({case_id,status,mapping_behavior}),all_passed:all(.[];.status == "passed")}' "$suite_artifacts/httppathexists/provenance.json" "$suite_artifacts/httppathnotexists/provenance.json" > "$suite_artifacts/normalized.json"
  first_provenance="$suite_artifacts/httppathexists/provenance.json"
  jq -n --arg ontop_commit "$(jq -r .ontop_commit "$first_provenance")" --arg postgres_digest "$(jq -r .postgres_image_digest "$first_provenance")" --arg required_postgres_digest "$(jq -r .required_postgres_image_digest "$first_provenance")" --arg ontop_raw_sha256 "$(sha256sum "$suite_artifacts/ontop.raw" | awk '{print $1}')" --arg rtop_raw_sha256 "$(sha256sum "$suite_artifacts/rtop.raw" | awk '{print $1}')" --arg normalized_sha256 "$(sha256sum "$suite_artifacts/normalized.json" | awk '{print $1}')" '{schema_version:1,case_id:"differential-postgres-property-path-exists-endpoint",mapping_behavior:"PostgreSQL bounded property path and correlated EXISTS/NOT EXISTS",status:"passed",status_reason:"固定 PostgreSQL 17 digest 下的双边 HTTP JSON 结果一致",ontop_commit:$ontop_commit,postgres_image_digest:$postgres_digest,required_postgres_image_digest:$required_postgres_digest,fixture:"tests/compat/postgres-http/{algebra-init.sql,mapping-algebra-bag.obda,property-path-exists.rq,not-exists-correlation.rq}",query:"property path EXISTS and correlated NOT EXISTS",postgres_extensions:"none",independent_processes:{ontop:"Java 17 container endpoint",rtop:"Rust host endpoint"},sha256:{ontop_raw:$ontop_raw_sha256,rtop_raw:$rtop_raw_sha256,normalized:$normalized_sha256}}' > "$suite_artifacts/provenance.json"
  printf '%s\n' "differential httppath/suite: passed; artifacts=$suite_artifacts"
  exit 0
fi

case "$case_name" in
  d001)
case "$variant" in
  template)
    mapping_file=r2rmla.ttl
    mapping_behavior='template IRI subject'
    student_kind=iri
    artifact_case_id=differential-r2rml-d001-template
    ;;
  blank-node)
    mapping_file=r2rmlb.ttl
    mapping_behavior='template blank-node subject'
    student_kind=blank-node
    artifact_case_id=differential-r2rml-d001-blank-node
    ;;
  *)
    printf '%s\n' "未知 DIFFERENTIAL_D001_VARIANT：$variant" >&2
    exit 64
    ;;
esac
    source_case=D001
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-d001/init.sql"
    query="$root/tests/compat/postgres-d001/query.rq"
    comparison=query
    postgres_extensions=none
    ;;
  d002a)
    mapping_file=r2rmla.ttl
    mapping_behavior='multiple predicate-object maps and rr:class'
    artifact_case_id=differential-r2rml-d002-multiple-pom-class
    source_case=D002
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D002/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d002b)
    mapping_file=r2rmlb.ttl
    mapping_behavior='template blank-node subject'
    artifact_case_id=differential-r2rml-d002-blank-node-subject
    source_case=D002
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D002/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=pgcrypto
    ;;
  d002d)
    mapping_file=r2rmld.ttl
    mapping_behavior='SQL projection blank-node subject'
    artifact_case_id=differential-r2rml-d002-sql-projection-blank-node
    source_case=D002
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D002/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d002c)
    mapping_file=r2rmlc.ttl
    mapping_behavior='reject undefined object SQL column'
    artifact_case_id=differential-r2rml-d002-reject-undefined-object-column
    source_case=D002
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D002/create.sql"
    query=''
    comparison=rejection
    postgres_extensions=none
    ;;
  d002e)
    mapping_file=r2rmle.ttl
    mapping_behavior='reject undefined SQL table'
    artifact_case_id=differential-r2rml-d002-reject-undefined-table
    source_case=D002
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D002/create.sql"
    query=''
    comparison=rejection
    postgres_extensions=none
    ;;
  d002f)
    mapping_file=r2rmlf.ttl
    mapping_behavior='reject unquoted mixed-case SQL identifiers'
    artifact_case_id=differential-r2rml-d002-reject-unquoted-identifiers
    source_case=D002
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D002/create.sql"
    query=''
    comparison=rejection
    postgres_extensions=none
    ;;
  d002h)
    mapping_file=r2rmlh.ttl
    mapping_behavior='reject duplicate SQL projection aliases'
    artifact_case_id=differential-r2rml-d002-reject-duplicate-alias
    source_case=D002
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D002/create.sql"
    query=''
    comparison=rejection
    postgres_extensions=none
    ;;
  d002g)
    mapping_file=r2rmlg.ttl
    mapping_behavior='reject invalid SQL logical table source'
    artifact_case_id=differential-r2rml-d002-invalid-sql
    source_case=D002
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D002/create.sql"
    query=''
    comparison=rejection
    postgres_extensions=none
    ;;
  d002i)
    mapping_file=r2rmli.ttl
    mapping_behavior='rr:sqlVersion SQL2008 on PostgreSQL logical table'
    artifact_case_id=differential-r2rml-d002-sql-version
    source_case=D002
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D002/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d002j)
    mapping_file=r2rmlj.ttl
    mapping_behavior='qualified SQL column projection'
    artifact_case_id=differential-r2rml-d002-qualified-sql-columns
    source_case=D002
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D002/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d012c)
    mapping_file=r2rmlc.ttl
    mapping_behavior='reject TriplesMap without rr:subjectMap'
    artifact_case_id=differential-r2rml-d012-rejects-missing-subject-map
    source_case=D012
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D012/create.sql"
    query=''
    comparison=rejection
    postgres_extensions=pgcrypto
    ;;
  d012d)
    mapping_file=r2rmld.ttl
    mapping_behavior='reject TriplesMap with multiple rr:subjectMap values'
    artifact_case_id=differential-r2rml-d012-rejects-multiple-subject-maps
    source_case=D012
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D012/create.sql"
    query=''
    comparison=rejection
    postgres_extensions=pgcrypto
    ;;
  d007a)
    mapping_file=r2rmla.ttl
    mapping_behavior='explicit rdf:type predicate-object map'
    artifact_case_id=differential-r2rml-d007-explicit-rdf-type-pom
    source_case=D007
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D007/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d007c)
    mapping_file=r2rmlc.ttl
    mapping_behavior='multiple rr:class values on a subject map'
    artifact_case_id=differential-r2rml-d007-multiple-subject-classes
    source_case=D007
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D007/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d007h)
    mapping_file=r2rmlh.ttl
    mapping_behavior='reject literal-valued column graphMap'
    artifact_case_id=differential-r2rml-d007-reject-literal-column-graph
    source_case=D007
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D007/create.sql"
    query=''
    comparison=rejection
    postgres_extensions=none
    ;;
  d003c)
    mapping_file=r2rmlc.ttl
    mapping_behavior='literal object template'
    artifact_case_id=differential-r2rml-d003-literal-object-template
    source_case=D003
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D003/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d003a)
    mapping_file=r2rmla.ttl
    mapping_behavior='rr:SQL1979 annotation accepted by the fixed Ontop CLI baseline'
    artifact_case_id=differential-r2rml-d003-sql1979-annotation
    source_case=D003
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D003/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d003b)
    mapping_file=r2rmlb.ttl
    mapping_behavior='SQL concatenation literal object'
    artifact_case_id=differential-r2rml-d003-sql-concatenated-literal
    source_case=D003
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D003/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d005a)
    mapping_file=r2rmla.ttl
    mapping_behavior='template IRI, rr:class, and FLOAT object'
    artifact_case_id=differential-r2rml-d005-template-iri-class-float
    source_case=D005
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D005/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d005b)
    mapping_file=r2rmlb.ttl
    mapping_behavior='multi-column blank-node subject template'
    artifact_case_id=differential-r2rml-d005-multi-column-blank-node-subject
    source_case=D005
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D005/create.sql"
    query=''
    comparison=materialize
    # Ontop 为 rr:BlankNode template 生成 PostgreSQL digest() 调用。
    postgres_extensions=pgcrypto
    ;;
  d006a)
    mapping_file=r2rmla.ttl
    mapping_behavior='constant subject, predicate, object, and named graph maps'
    artifact_case_id=differential-r2rml-d006-constant-subject-predicate-graph-map
    source_case=D006
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D006/create.sql"
    query="$root/tests/compat/postgres-d006/differential-a.rq"
    comparison=http-error
    postgres_extensions=none
    ;;
  d007b)
    mapping_file=r2rmlb.ttl
    mapping_behavior='subject-map named graph with rdf:type and column object'
    artifact_case_id=differential-r2rml-d007-direct-named-graph
    source_case=D007
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D007/create.sql"
    query="$root/tests/compat/postgres-d007/differential-b.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  d007d)
    mapping_file=r2rmld.ttl
    mapping_behavior='multiple explicit rdf:type predicate-object maps'
    artifact_case_id=differential-r2rml-d007-multiple-explicit-rdf-type-pom
    source_case=D007
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D007/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d007e)
    mapping_file=r2rmle.ttl
    mapping_behavior='named graph subject rr:class'
    artifact_case_id=differential-r2rml-d007-named-graph-subject-class
    source_case=D007
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D007/create.sql"
    query="$root/tests/compat/postgres-d007/differential-b.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  d007f)
    mapping_file=r2rmlf.ttl
    mapping_behavior='named graph explicit rdf:type predicate-object map'
    artifact_case_id=differential-r2rml-d007-named-graph-explicit-rdf-type
    source_case=D007
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D007/create.sql"
    query="$root/tests/compat/postgres-d007/differential-b.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  d007g)
    mapping_file=r2rmlg.ttl
    mapping_behavior='explicit rr:defaultGraph'
    artifact_case_id=differential-r2rml-d007-explicit-default-graph
    source_case=D007
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D007/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d008a)
    mapping_file=r2rmla.ttl
    mapping_behavior='template named graph'
    artifact_case_id=differential-r2rml-d008-template-named-graph
    source_case=D008
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D008/create.sql"
    query="$root/tests/compat/postgres-d007/differential-b.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  d008b)
    mapping_file=r2rmlb.ttl
    mapping_behavior='ref-object map without join condition'
    artifact_case_id=differential-r2rml-d008-ref-object-map-without-join
    source_case=D008
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D008/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d008c)
    mapping_file=r2rmlc.ttl
    mapping_behavior='multiple predicate maps in one predicate-object map'
    artifact_case_id=differential-r2rml-d008-multiple-predicate-maps
    source_case=D008
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D008/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d009a)
    mapping_file=r2rmla.ttl
    mapping_behavior='ref-object map join condition'
    artifact_case_id=differential-r2rml-d009-ref-object-map-join
    source_case=D009
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D009/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d009b)
    mapping_file=r2rmlb.ttl
    mapping_behavior='subject-map and predicate-object-map named graphs'
    artifact_case_id=differential-r2rml-d009-subject-and-pom-named-graphs
    source_case=D009
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D009/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d009c)
    mapping_file=r2rmlc.ttl
    mapping_behavior='unnamed SQL projection column'
    artifact_case_id=differential-r2rml-d009-unnamed-sql-projection-column
    source_case=D009
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D009/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ontop_infer_default_datatype=true
    ;;
  d009d)
    mapping_file=r2rmld.ttl
    mapping_behavior='named SQL projection column'
    artifact_case_id=differential-r2rml-d009-named-sql-projection-column
    source_case=D009
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D009/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ontop_infer_default_datatype=true
    # 固定 Ontop 的该 property 组合将 SQL COUNT alias 输出为 simple literal。
    rtop_mapping_infer_default_datatype=false
    ;;
  d010a)
    mapping_file=r2rmla.ttl
    mapping_behavior='quoted special-character column template'
    artifact_case_id=differential-r2rml-d010-single-column-template
    source_case=D010
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D010/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d010b)
    mapping_file=r2rmlb.ttl
    mapping_behavior='multi-column IRI template component percent-encoding'
    artifact_case_id=differential-r2rml-d010-iri-template-percent-encoding
    source_case=D010
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D010/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d010c)
    mapping_file=r2rmlc.ttl
    mapping_behavior='escaped literal template braces'
    artifact_case_id=differential-r2rml-d010-escaped-literal-template
    source_case=D010
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D010/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d011a)
    mapping_file=r2rmla.ttl
    mapping_behavior='three-table SQL view many-to-many relation'
    artifact_case_id=differential-r2rml-d011-many-to-many-sql-view
    source_case=D011
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D011/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d011b)
    mapping_file=r2rmlb.ttl
    mapping_behavior='additional LinkMap many-to-many relation'
    artifact_case_id=differential-r2rml-d011-many-to-many-link-map
    source_case=D011
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D011/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d013a)
    mapping_file=r2rmla.ttl
    mapping_behavior='NULL template column omits RDF triple'
    artifact_case_id=differential-r2rml-d013-null-template-omits-triple
    source_case=D013
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D013/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d014a)
    mapping_file=r2rmla.ttl
    mapping_behavior='inverseExpression blank-node subject map'
    artifact_case_id=differential-r2rml-d014-inverse-expression-blank-node
    source_case=D014
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D014/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d014b)
    mapping_file=r2rmlb.ttl
    mapping_behavior='RefObjectMap join with inline IRI and literal terms'
    artifact_case_id=differential-r2rml-d014-ref-object-map-and-inline-terms
    source_case=D014
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D014/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d014c)
    mapping_file=r2rmlc.ttl
    mapping_behavior='named ObjectMap positiveInteger datatype and RefObjectMap join'
    artifact_case_id=differential-r2rml-d014-named-object-map-and-datatype
    source_case=D014
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D014/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d014d)
    mapping_file=r2rmld.ttl
    mapping_behavior='CASE job SQL logical table role IRI'
    artifact_case_id=differential-r2rml-d014-case-role-sql-logical-table
    source_case=D014
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D014/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d015a)
    mapping_file=r2rmla.ttl
    mapping_behavior='column language tags from two SQL logical tables'
    artifact_case_id=differential-r2rml-d015-column-language-tags
    source_case=D015
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D015/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d015b)
    mapping_file=r2rmlb.ttl
    mapping_behavior='reject invalid rr:language values'
    artifact_case_id=differential-r2rml-d015-reject-invalid-language-tag
    source_case=D015
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D015/create.sql"
    query=''
    comparison=rejection
    postgres_extensions=none
    ;;
  d016c)
    mapping_file=r2rmlc.ttl
    mapping_behavior='PostgreSQL DATE and TIMESTAMP default SQL datatypes'
    artifact_case_id=differential-r2rml-d016-date-timestamp
    source_case=D016
    init_file=r2rml-init.sql
    init_sql="$root/tests/compat/postgres-d016/r2rml-init.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D016/r2rmlc.ttl; tests/compat/postgres-d016/r2rml-init.sql（PostgreSQL BYTEA/DOUBLE PRECISION 适配）'
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d016d)
    mapping_file=r2rmld.ttl
    mapping_behavior='PostgreSQL BOOLEAN default SQL datatype'
    artifact_case_id=differential-r2rml-d016-boolean
    source_case=D016
    init_file=r2rml-init.sql
    init_sql="$root/tests/compat/postgres-d016/r2rml-init.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D016/r2rmld.ttl; tests/compat/postgres-d016/r2rml-init.sql（PostgreSQL BYTEA/DOUBLE PRECISION 适配）'
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d016a)
    mapping_file=r2rmla.ttl
    mapping_behavior='PostgreSQL string and INTEGER default SQL datatypes'
    artifact_case_id=differential-r2rml-d016-string-integer
    source_case=D016
    init_file=r2rml-init.sql
    init_sql="$root/tests/compat/postgres-d016/r2rml-init.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D016/r2rmla.ttl; tests/compat/postgres-d016/r2rml-init.sql（PostgreSQL BYTEA/DOUBLE PRECISION 适配）'
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d016b)
    mapping_file=r2rmlb.ttl
    mapping_behavior='PostgreSQL REAL and FLOAT default SQL datatypes'
    artifact_case_id=differential-r2rml-d016-real-float
    source_case=D016
    init_file=r2rml-init.sql
    init_sql="$root/tests/compat/postgres-d016/r2rml-init.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D016/r2rmlb.ttl; tests/compat/postgres-d016/r2rml-init.sql（PostgreSQL BYTEA/DOUBLE PRECISION 适配）'
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d016e)
    mapping_file=r2rmle.ttl
    mapping_behavior='PostgreSQL BYTEA rendered into data IRI template'
    artifact_case_id=differential-r2rml-d016-binary-data-iri
    source_case=D016
    init_file=r2rml-init.sql
    init_sql="$root/tests/compat/postgres-d016/r2rml-init.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D016/r2rmle.ttl; tests/compat/postgres-d016/r2rml-init.sql（PostgreSQL BYTEA 适配）'
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d020a)
    mapping_file=r2rmla.ttl
    mapping_behavior='IRI template component percent-encoding'
    artifact_case_id=differential-r2rml-d020-iri-template-component-encoding
    source_case=D020
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D020/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d020b)
    mapping_file=r2rmlb.ttl
    mapping_behavior='reject runtime unescaped IRI column data'
    artifact_case_id=differential-r2rml-d020-invalid-iri-column-data
    source_case=D020
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D020/create.sql"
    query=''
    comparison=rejection
    postgres_extensions=none
    ;;
  d019a)
    mapping_file=r2rmla.ttl
    mapping_behavior='fixed Ontop CLI rejects relative IRI column values as non-absolute'
    artifact_case_id=differential-r2rml-d019-relative-iri-column-base
    source_case=D019
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D019/create.sql"
    query=''
    comparison=rejection
    rtop_mapping_require_absolute_iri_values=true
    postgres_extensions=none
    ;;
  d019b)
    mapping_file=r2rmlb.ttl
    mapping_behavior='reject runtime invalid IRI column data'
    artifact_case_id=differential-r2rml-d019-invalid-iri-column-data
    source_case=D019
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D019/create.sql"
    query=''
    comparison=rejection
    postgres_extensions=none
    ;;
  d026a)
    mapping_file=r2rmla.ttl
    mapping_behavior='second-level RefObjectMap join Student Sport SportType'
    artifact_case_id=differential-r2rml-d026-second-level-ref-object-map
    source_case=D026
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D026/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d018a)
    mapping_file=r2rmla.ttl
    mapping_behavior='CHAR fixed-width literal lexical form'
    artifact_case_id=differential-r2rml-d018-char-fixed-width-literal
    source_case=D018
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D018/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d004a)
    mapping_file=r2rmla.ttl
    mapping_behavior='two TriplesMaps with rr:class rdf:type'
    artifact_case_id=differential-r2rml-d004-multiple-triples-map-class
    source_case=D004
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D004/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  d012a)
    mapping_file=r2rmla.ttl
    mapping_behavior='duplicate tuple preserves blank-node identity'
    artifact_case_id=differential-r2rml-d012-duplicate-row-blank-node
    source_case=D012
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D012/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=pgcrypto
    ;;
  d012b)
    mapping_file=r2rmlb.ttl
    mapping_behavior='cross-TriplesMap blank-node identity'
    artifact_case_id=differential-r2rml-d012-cross-map-blank-node
    source_case=D012
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D012/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=pgcrypto
    ;;
  d012e)
    mapping_file=r2rmle.ttl
    mapping_behavior='no-primary-key tables mapped to blank-node default mapping'
    artifact_case_id=differential-r2rml-d012-default-mapping-no-primary-key
    source_case=D012
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D012/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=pgcrypto
    ;;
  d000)
    mapping_file=r2rml.ttl
    mapping_behavior='empty logical table produces empty RDF graph'
    artifact_case_id=differential-r2rml-d000-empty-logical-table
    source_case=D000
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D000/create.sql"
    query=''
    comparison=materialize
    postgres_extensions=pgcrypto
    ;;
  climaterialize)
    mapping_file=''
    mapping_source="$baseline/client/cli/src/test/resources/test/simplemapping.obda"
    mapping_behavior='CLI materialize native OBDA graph as N-Quads'
    artifact_case_id=differential-cli-materialize
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-cli-materialize/init.sql"
    fixture_init_provenance='client/cli/src/test/resources/test/simplemapping.obda; tests/compat/postgres-cli-materialize/init.sql; Ontop CLI materialize + rtop CLI materialize'
    query=''
    comparison=materialize
    postgres_extensions=none
    ;;
  clicompile)
    mapping_file=''
    mapping_source="$baseline/client/cli/src/test/resources/test/simplemapping.obda"
    mapping_behavior='CLI compile is a zero-output compatibility no-op'
    artifact_case_id=differential-cli-compile
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-cli-materialize/init.sql"
    fixture_init_provenance='client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopCompile.java; client/cli/src/test/resources/test/simplemapping.obda; rtop valid config'
    query='compile stdout'
    comparison=compile
    postgres_extensions=none
    ;;
  clivalidate)
    mapping_file=''
    mapping_source="$baseline/client/cli/src/test/resources/test/simplemapping.obda"
    ontology_source="$baseline/client/cli/src/test/resources/test/simplemapping.owl"
    mapping_behavior='CLI validate loads ontology and native OBDA mapping then reports completion'
    artifact_case_id=differential-cli-validate
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-cli-materialize/init.sql"
    fixture_init_provenance='client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopValidate.java; client/cli/src/test/resources/test/{simplemapping.obda,simplemapping.owl}; tests/compat/postgres-cli-materialize/init.sql'
    query='validate stdout'
    comparison=validate
    postgres_extensions=none
    ;;
  clisimplemapping)
    mapping_file=''
    mapping_source="$baseline/client/cli/src/test/resources/test/simplemapping.obda"
    mapping_behavior='simplemapping native OBDA SELECT BGP returns the mapped A resource'
    artifact_case_id=differential-cli-simplemapping-select-bgp
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-cli-materialize/init.sql"
    fixture_init_provenance='client/cli/src/test/resources/test/simplemapping.obda; tests/compat/postgres-cli-materialize/{init.sql,select-a.rq}'
    query="$root/tests/compat/postgres-cli-materialize/select-a.rq"
    comparison=simple-csv
    postgres_extensions=none
    ;;
  cliquery)
    mapping_file=''
    mapping_source="$baseline/client/cli/src/test/resources/test/simplemapping.obda"
    mapping_behavior='CLI query executes a PostgreSQL-backed SELECT and exposes an IRI binding'
    artifact_case_id=differential-cli-query
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-cli-materialize/init.sql"
    fixture_init_provenance='client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopQuery.java; client/cli/src/test/resources/test/simplemapping.obda; tests/compat/postgres-cli-materialize/{init.sql,select-a.rq}'
    query="$root/tests/compat/postgres-cli-materialize/select-a.rq"
    comparison=simple-csv
    postgres_extensions=none
    ;;
  cliendpoint)
    mapping_file=''
    mapping_source="$baseline/client/cli/src/test/resources/test/simplemapping.obda"
    mapping_behavior='CLI endpoint exposes a PostgreSQL-backed SPARQL JSON SELECT result'
    artifact_case_id=differential-cli-endpoint
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-cli-materialize/init.sql"
    fixture_init_provenance='client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopEndpoint.java; client/cli/src/test/resources/test/simplemapping.obda; tests/compat/postgres-cli-materialize/{init.sql,select-a.rq}'
    query="$root/tests/compat/postgres-cli-materialize/select-a.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  clipretty)
    mapping_file=r2rmla.ttl
    mapping_behavior='pretty-r2rml output reloads to the same PostgreSQL-backed RDF graph'
    artifact_case_id=differential-cli-pretty-r2rml
    source_case=D001
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D001/create.sql"
    fixture_init_provenance='client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopR2RMLPrettify.java; test/rdb2rdf-compliance/src/test/resources/D001/{create.sql,r2rmla.ttl}; each formatted mapping is reloaded by its own CLI'
    query='pretty-r2rml generated mappings materialized as N-Quads'
    comparison=pretty-materialize
    postgres_extensions=none
    ;;
  cliv1duplicatealias)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-query-kinds/v1-to-v3-duplicate-alias.obda"
    mapping_behavior='v1-to-v3 generates distinct stable aliases for duplicate qualified projection suffixes on reload'
    artifact_case_id=differential-cli-v1-to-v3-duplicate-qualified-alias
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-query-kinds/init.sql"
    fixture_init_provenance='client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopMappingV1ToV3.java; tests/compat/postgres-query-kinds/{init.sql,v1-to-v3-duplicate-alias.obda}; each converted mapping is reloaded by its own CLI'
    query='v1-to-v3 duplicate qualified aliases generated mappings materialized as N-Quads'
    comparison=v1-to-v3-materialize
    postgres_extensions=none
    ;;
  cliv1simplify)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-query-kinds/v1-to-v3-simplify-projection.obda"
    mapping_behavior='v1-to-v3 --simplify-projection replaces a simple PostgreSQL projection while preserving reload semantics'
    artifact_case_id=differential-cli-v1-to-v3-simplify-projection
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-query-kinds/init.sql"
    fixture_init_provenance='client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopMappingV1ToV3.java; tests/compat/postgres-query-kinds/{init.sql,v1-to-v3-simplify-projection.obda}; each converted mapping is reloaded by its own CLI'
    query='v1-to-v3 simplify-projection generated mappings materialized as N-Quads'
    comparison=v1-to-v3-materialize
    v1_option=--simplify-projection
    postgres_extensions=none
    ;;
  cliv1r2rmlalias)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-query-kinds/v1-to-v3-r2rml.ttl"
    mapping_name=mapping.ttl
    mapping_behavior='v1-to-v3 rewrites qualified R2RML template columns to SQL aliases while preserving reload semantics'
    artifact_case_id=differential-cli-v1-to-v3-r2rml-qualified-alias
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-query-kinds/init.sql"
    fixture_init_provenance='client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopMappingV1ToV3.java; tests/compat/postgres-query-kinds/{init.sql,v1-to-v3-r2rml.ttl}; each converted mapping is reloaded by its own CLI'
    query='v1-to-v3 R2RML qualified aliases generated mappings materialized as N-Quads'
    comparison=v1-to-v3-materialize
    postgres_extensions=none
    ;;
  cliv1native)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-query-kinds/v1-to-v3.obda"
    mapping_behavior='v1-to-v3 rejects legacy native SourceDeclaration containing sourceUri'
    artifact_case_id=differential-cli-v1-to-v3-native-obda
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-query-kinds/init.sql"
    fixture_init_provenance='client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopMappingV1ToV3.java; tests/compat/postgres-query-kinds/{init.sql,v1-to-v3.obda}; each CLI executes v1-to-v3 directly'
    query='v1-to-v3 rejects legacy sourceUri input'
    comparison=v1-to-v3-rejection
    postgres_extensions=none
    ;;
  clitoobda)
    mapping_file=r2rmla.ttl
    mapping_behavior='to-obda output reloads to the same PostgreSQL-backed RDF graph'
    artifact_case_id=differential-cli-r2rml-to-obda-d001-reload
    source_case=D001
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D001/create.sql"
    fixture_init_provenance='client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopR2RMLToOBDA.java; test/rdb2rdf-compliance/src/test/resources/D001/{create.sql,r2rmla.ttl}; each converted mapping is reloaded by its own CLI'
    query='to-obda generated mappings materialized as N-Quads'
    comparison=to-obda-materialize
    postgres_extensions=none
    ;;
  clitoobdarelativeiri)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-query-kinds/relative-r2rml.ttl"
    mapping_name=mapping.ttl
    mapping_behavior='to-obda uses Ontop default base for relative IRI templates while Turtle @base resolves RDF resources on reload'
    artifact_case_id=differential-cli-r2rml-to-obda-relative-iri
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-query-kinds/init.sql"
    fixture_init_provenance='client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopR2RMLToOBDA.java; tests/compat/postgres-query-kinds/{init.sql,relative-r2rml.ttl}; each converted mapping is reloaded by its own CLI'
    query='to-obda relative IRI generated mappings materialized as N-Quads'
    comparison=to-obda-materialize
    postgres_extensions=none
    ;;
  clitor2rml)
    mapping_file=''
    mapping_source="$baseline/client/cli/src/test/resources/test/simplemapping.obda"
    mapping_behavior='to-r2rml --force output reloads to the same PostgreSQL-backed RDF graph'
    artifact_case_id=differential-cli-obda-to-r2rml-force-reload
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-cli-materialize/init.sql"
    fixture_init_provenance='client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopOBDAToR2RML.java; client/cli/src/test/resources/test/simplemapping.obda; tests/compat/postgres-cli-materialize/init.sql; each converted mapping is reloaded by its own CLI'
    query='to-r2rml generated mappings materialized as N-Quads'
    comparison=to-r2rml-materialize
    postgres_extensions=none
    ;;
  clitor2rmlnamed)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-query-kinds/named-graph.obda"
    mapping_behavior='to-r2rml --force preserves a native constant named graph on reload'
    artifact_case_id=differential-cli-obda-to-r2rml-force-named-graph
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-query-kinds/init.sql"
    fixture_init_provenance='client/cli/src/test/resources/mapping-northwind-named-graph.obda; tests/compat/postgres-query-kinds/{init.sql,named-graph.obda}; each converted mapping is reloaded by its own CLI'
    query='to-r2rml named graph generated mappings materialized as N-Quads'
    comparison=to-r2rml-materialize
    postgres_extensions=none
    ;;
  clitor2rmltemplate)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-query-kinds/template-graph.obda"
    mapping_behavior='to-r2rml --force preserves a native template named graph on reload'
    artifact_case_id=differential-cli-obda-to-r2rml-force-template-graph
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-query-kinds/init.sql"
    fixture_init_provenance='client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopOBDAToR2RML.java; tests/compat/postgres-query-kinds/{init.sql,template-graph.obda}; each converted mapping is reloaded by its own CLI'
    query='to-r2rml template graph generated mappings materialized as N-Quads'
    comparison=to-r2rml-materialize
    postgres_extensions=none
    ;;
  clitor2rmltypedbnode)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-query-kinds/typed-blank-node.obda"
    mapping_behavior='to-r2rml --force preserves a native blank-node subject and typed column object on reload'
    artifact_case_id=differential-cli-obda-to-r2rml-typed-blank-node
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-query-kinds/init.sql"
    fixture_init_provenance='client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopOBDAToR2RML.java; tests/compat/postgres-query-kinds/{init.sql,typed-blank-node.obda}; each converted mapping is reloaded by its own CLI'
    query='to-r2rml typed blank-node generated mappings materialized as N-Quads'
    comparison=to-r2rml-materialize
    postgres_extensions=pgcrypto
    ;;
  clitor2rmllegacysource)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-query-kinds/v1-to-v3.obda"
    mapping_behavior='to-r2rml rejects a native mapping that still declares SourceDeclaration'
    artifact_case_id=differential-cli-obda-to-r2rml-legacy-source-declaration-error
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-query-kinds/init.sql"
    fixture_init_provenance='client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopOBDAToR2RML.java; tests/compat/postgres-query-kinds/{init.sql,v1-to-v3.obda}; each CLI executes to-r2rml --force directly'
    query='to-r2rml rejects legacy SourceDeclaration input'
    comparison=conversion-rejection
    postgres_extensions=none
    ;;
  clitor2rmlduplicateid)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-query-kinds/duplicate-mapping-id.obda"
    mapping_behavior='to-r2rml rejects duplicate native mappingId values'
    artifact_case_id=differential-cli-obda-to-r2rml-duplicate-mapping-id
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-query-kinds/init.sql"
    fixture_init_provenance='test/docker-tests/src/test/resources/issue461/mappingwithduplicates.obda; tests/compat/postgres-query-kinds/{init.sql,duplicate-mapping-id.obda}; each CLI executes to-r2rml --force directly'
    query='to-r2rml rejects duplicate native mappingId values'
    comparison=conversion-rejection
    postgres_extensions=none
    ;;
  clibootstrap)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-query-kinds/mapping.obda"
    mapping_behavior='bootstrap generated native mappings materialize the PostgreSQL catalog graph equivalently'
    artifact_case_id=differential-cli-bootstrap
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-query-kinds/init.sql"
    fixture_init_provenance='client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopBootstrap.java; tests/compat/postgres-query-kinds/init.sql; each generated OBDA is reloaded by its own CLI'
    query='bootstrap generated mappings materialized as N-Quads'
    comparison=bootstrap-materialize
    bootstrap_base='https://bootstrap.example'
    postgres_extensions=none
    ;;
  cliextractmetadata)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-query-kinds/mapping.obda"
    mapping_behavior='extract PostgreSQL relations, columns, primary keys, and nullable metadata as JSON'
    artifact_case_id=differential-cli-extract-db-metadata
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-query-kinds/init.sql"
    fixture_init_provenance='client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopExtractDBMetadata.java; tests/compat/postgres-query-kinds/init.sql'
    query='PostgreSQL metadata JSON'
    comparison=metadata
    postgres_extensions=none
    ;;
  lubm)
    lubm_query=${LUBM_QUERY:-1}
    case "$lubm_query" in
      1|2|3|4|5|6|7|8|9|10|11|12|13|14) ;;
      *)
        printf '%s\n' "LUBM_QUERY 必须是 1..14，实际为：$lubm_query" >&2
        exit 64
        ;;
    esac
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/lubm/lubm-pgsql.obda"
    ontology_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/lubm/lubm.owl"
    mapping_behavior="LUBM PostgreSQL manifest query-$lubm_query 的 SPARQL JSON bindings"
    artifact_case_id="differential-postgres-lubm-query-$lubm_query"
    source_case=none
    init_file=fixture.sql
    init_sql="$root/tests/compat/postgres-lubm/fixture.sql"
    fixture_init_provenance="test/docker-tests/src/test/resources/testcases-docker/virtual-mode/lubm/{lubm-pgsql.obda,lubm.owl,query-$lubm_query.rq}; tests/compat/postgres-lubm/fixture.sql"
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/lubm/query-$lubm_query.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  suiteask)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/sparql/ask/stockexchange-pgsql.obda"
    ontology_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/sparql/ask/stockexchange.owl"
    mapping_behavior='DockerPostgresTestSuite stockexchange ASK manifest query 的 SPARQL JSON boolean'
    artifact_case_id=differential-postgres-suite-manifest-ask-ask
    source_case=none
    init_file=stockexchange-create-pgsql.sql
    init_sql="$baseline/test/docker-tests/src/test/resources/dump/stockexchange-create-pgsql.sql"
    fixture_init_provenance='test/docker-tests/src/test/resources/testcases-docker/sparql/ask/{stockexchange-pgsql.obda,stockexchange.owl,ask.rq}; test/docker-tests/src/test/resources/dump/stockexchange-create-pgsql.sql'
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/sparql/ask/ask.rq"
    comparison=http-ask
    postgres_extensions=none
    init_stockexchange_dump=true
    ;;
  suitefilterbooleanand)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/filters/stockexchange-pgsql.obda"
    ontology_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/filters/stockexchange.owl"
    mapping_behavior='DockerPostgresTestSuite stockexchange filters boolean-and manifest query 的 SPARQL JSON bindings'
    artifact_case_id=differential-postgres-suite-manifest-filters-boolean-and
    source_case=none
    init_file=stockexchange-create-pgsql.sql
    init_sql="$baseline/test/docker-tests/src/test/resources/dump/stockexchange-create-pgsql.sql"
    fixture_init_provenance='test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/filters/{stockexchange-pgsql.obda,stockexchange.owl,boolean-and.rq}; test/docker-tests/src/test/resources/dump/stockexchange-create-pgsql.sql'
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/filters/boolean-and.rq"
    comparison=http-json
    postgres_extensions=none
    init_stockexchange_dump=true
    ;;
  suitefilter)
    suite_filter_query=${SUITE_FILTER_QUERY:-}
    case "$suite_filter_query" in
      boolean-and|boolean-or|boolean-nested-1|boolean-nested-2|boolean-nested-3|boolean-eq|boolean-neq|string-eq|string-neq|integer-eq|integer-neq|integer-gt|integer-gte|integer-lt|integer-lte|decimal-eq|decimal-neq|decimal-gt|decimal-gte|decimal-lt|decimal-lte|double-eq|double-neq|double-gt|double-gte|double-lt|double-lte|datetime-eq|datetime-neq|datetime-gt|datetime-gte|datetime-lt|datetime-lte|literal-langmatch) ;;
      *)
        printf '%s\n' "SUITE_FILTER_QUERY 必须是已登记的固定 filters manifest 原子，实际为：$suite_filter_query" >&2
        exit 64
        ;;
    esac
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/filters/stockexchange-pgsql.obda"
    ontology_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/filters/stockexchange.owl"
    mapping_behavior="DockerPostgresTestSuite stockexchange filters $suite_filter_query manifest query 的 SPARQL JSON bindings"
    artifact_case_id="differential-postgres-suite-manifest-filters-$suite_filter_query"
    source_case=none
    init_file=stockexchange-create-pgsql.sql
    init_sql="$baseline/test/docker-tests/src/test/resources/dump/stockexchange-create-pgsql.sql"
    fixture_init_provenance="test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/filters/{stockexchange-pgsql.obda,stockexchange.owl,$suite_filter_query.rq}; test/docker-tests/src/test/resources/dump/stockexchange-create-pgsql.sql"
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/filters/$suite_filter_query.rq"
    comparison=http-json
    postgres_extensions=none
    init_stockexchange_dump=true
    ;;
  suitedatatype)
    suite_datatype_query=${SUITE_DATATYPE_QUERY:-}
    case "$suite_datatype_query" in
      literal-1|literal-2|string-1|string-2|string-3|integer-1|integer-2|integer-3|pos-integer-1|pos-integer-2|pos-integer-2-bis|pos-integer-3|pos-integer-3-bis|neg-integer-1|neg-integer-2|neg-integer-3|decimal-1|decimal-2|decimal-2-bis|decimal-3|decimal-3-bis|pos-decimal-1|pos-decimal-2|pos-decimal-3|neg-decimal-1|neg-decimal-2|neg-decimal-3|double-1|double-2|double-3|pos-double-1|pos-double-2|pos-double-3|neg-double-1|neg-double-2|neg-double-3|datetime-1a|datetime-1b|datetime-1c|datetime-2a|datetime-2b|datetime-2c|datetime-3a|datetime-3b|datetime-3c|datetime-3d|datetime-3e|datetime-3f|datetime-3g|datetime-3h|datetime-3i|datetime-3j|boolean-1a|boolean-1b|boolean-1c|boolean-1d|boolean-2a|boolean-2b|boolean-2c|boolean-2d|boolean-3a|boolean-3b|boolean-3c|boolean-3d|boolean-3e|boolean-3f) ;;
      *)
        printf '%s\n' "SUITE_DATATYPE_QUERY 必须是已登记的固定 datatypes manifest 原子，实际为：$suite_datatype_query" >&2
        exit 64
        ;;
    esac
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/datatypes/stockexchange-pgsql.obda"
    ontology_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/datatypes/stockexchange.owl"
    mapping_behavior="DockerPostgresTestSuite stockexchange datatypes $suite_datatype_query manifest query 的 SPARQL JSON bindings"
    artifact_case_id="differential-postgres-suite-manifest-datatypes-$suite_datatype_query"
    source_case=none
    init_file=stockexchange-create-pgsql.sql
    init_sql="$baseline/test/docker-tests/src/test/resources/dump/stockexchange-create-pgsql.sql"
    fixture_init_provenance="test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/datatypes/{stockexchange-pgsql.obda,stockexchange.owl,$suite_datatype_query.rq}; test/docker-tests/src/test/resources/dump/stockexchange-create-pgsql.sql"
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/datatypes/$suite_datatype_query.rq"
    if [ "$suite_datatype_query" = boolean-3b ] || [ "$suite_datatype_query" = boolean-3c ]; then
      comparison=http-invalid-boolean-lexical
    elif [ "$suite_datatype_query" = datetime-3a ] || [ "$suite_datatype_query" = datetime-3b ] || [ "$suite_datatype_query" = datetime-3f ] || [ "$suite_datatype_query" = datetime-3g ] || [ "$suite_datatype_query" = datetime-3i ] || [ "$suite_datatype_query" = datetime-3j ]; then
      comparison=http-invalid-datetime-lexical
    elif [ "$suite_datatype_query" = string-2 ] || [ "$suite_datatype_query" = datetime-2a ] || [ "$suite_datatype_query" = datetime-2b ] || [ "$suite_datatype_query" = datetime-2c ] || [ "$suite_datatype_query" = boolean-2c ]; then
      comparison=http-parse-error
    else
      comparison=http-json
    fi
    postgres_extensions=none
    init_stockexchange_dump=true
    ;;
  suitemodifier)
    suite_modifier_query=${SUITE_MODIFIER_QUERY:-}
    case "$suite_modifier_query" in
      slice-test|limit-test|offset-test|limit0-offsetn|limitn-offset0|orderby-test|orderbydesc-test|orderbycombined-test|slice-orderby|orderbyliteral-test) ;;
      *)
        printf '%s\n' "SUITE_MODIFIER_QUERY 必须是已登记的固定 modifiers manifest 原子，实际为：$suite_modifier_query" >&2
        exit 64
        ;;
    esac
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/modifiers/stockexchange-pgsql.obda"
    ontology_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/modifiers/stockexchange.owl"
    mapping_behavior="DockerPostgresTestSuite stockexchange modifiers $suite_modifier_query manifest query 的 SPARQL JSON bindings"
    artifact_case_id="differential-postgres-suite-manifest-modifiers-$suite_modifier_query"
    source_case=none
    init_file=stockexchange-create-pgsql.sql
    init_sql="$baseline/test/docker-tests/src/test/resources/dump/stockexchange-create-pgsql.sql"
    fixture_init_provenance="test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/modifiers/{stockexchange-pgsql.obda,stockexchange.owl,$suite_modifier_query.rq}; test/docker-tests/src/test/resources/dump/stockexchange-create-pgsql.sql"
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/modifiers/$suite_modifier_query.rq"
    comparison=http-json
    postgres_extensions=none
    init_stockexchange_dump=true
    ;;
  suitesimplecq)
    suite_simplecq_query=${SUITE_SIMPLECQ_QUERY:-}
    case "$suite_simplecq_query" in
      addresses|addresses-id|person-addresses|stocktraders|brokers-workfor-themselves|brokers-workfor-physical|brokers-workfor-legal|brokers-workfor-legal-physical|transactions-finantialinstrument|transaction-stock-type|transaction-offer-stock) ;;
      *)
        printf '%s\n' "SUITE_SIMPLECQ_QUERY 必须是已登记的固定 simplecq manifest 原子，实际为：$suite_simplecq_query" >&2
        exit 64
        ;;
    esac
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/simplecq/stockexchange-pgsql.obda"
    ontology_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/simplecq/stockexchange.owl"
    mapping_behavior="DockerPostgresTestSuite stockexchange simplecq $suite_simplecq_query manifest query 的 SPARQL JSON bindings"
    artifact_case_id="differential-postgres-suite-manifest-simplecq-$suite_simplecq_query"
    source_case=none
    init_file=stockexchange-create-pgsql.sql
    init_sql="$baseline/test/docker-tests/src/test/resources/dump/stockexchange-create-pgsql.sql"
    fixture_init_provenance="test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/simplecq/{stockexchange-pgsql.obda,stockexchange.owl,$suite_simplecq_query.rq}; test/docker-tests/src/test/resources/dump/stockexchange-create-pgsql.sql"
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/stockexchange/simplecq/$suite_simplecq_query.rq"
    comparison=http-json
    postgres_extensions=none
    init_stockexchange_dump=true
    ;;
  httpontologydisabled)
    mapping_file=''
    mapping_source="$baseline/client/cli/src/test/resources/test/simplemapping.obda"
    mapping_behavior='endpoint does not expose /ontology while ontology download is disabled'
    artifact_case_id=differential-http-ontology-download-disabled-404
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-cli-materialize/init.sql"
    fixture_init_provenance='client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/OntologyFetcherController.java; client/cli/src/test/resources/test/simplemapping.obda; tests/compat/postgres-cli-materialize/init.sql'
    query='HTTP GET /ontology status'
    comparison=http-status
    postgres_extensions=none
    ;;
  httpontologynoontology)
    mapping_file=''
    mapping_source="$baseline/client/cli/src/test/resources/test/simplemapping.obda"
    mapping_behavior='enabled ontology download reports no ontology as HTTP 404 for GET and POST'
    artifact_case_id=differential-http-ontology-no-ontology-404
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-cli-materialize/init.sql"
    fixture_init_provenance='client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/OntologyFetcherController.java; client/cli/src/test/resources/test/simplemapping.obda; tests/compat/postgres-cli-materialize/init.sql'
    query='HTTP GET and POST /ontology without configured ontology'
    comparison=http-status-body
    endpoint_enable_download_ontology=true
    ontop_endpoint_options='--enable-download-ontology'
    postgres_extensions=none
    ;;
  httpontologyenabled)
    mapping_file=''
    mapping_source="$baseline/client/cli/src/test/resources/test/simplemapping.obda"
    ontology_source="$baseline/client/cli/src/test/resources/test/simplemapping.owl"
    mapping_behavior='enabled ontology download serves the configured ontology through GET and POST'
    artifact_case_id=differential-http-ontology
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-cli-materialize/init.sql"
    fixture_init_provenance='client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/OntologyFetcherController.java; client/cli/src/test/resources/test/{simplemapping.obda,simplemapping.owl}; tests/compat/postgres-cli-materialize/init.sql'
    query='HTTP GET and POST /ontology with configured ontology'
    comparison=http-ontology-content
    endpoint_enable_download_ontology=true
    ontop_endpoint_options='--enable-download-ontology -t /case/ontology.owl'
    postgres_extensions=none
    ;;
  httppredefined)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-http/predefined-person.obda"
    mapping_behavior='predefined GRAPH route binds an IRI parameter and returns Turtle'
    artifact_case_id=differential-http-predefined
    source_case=none
    init_file=algebra-init.sql
    init_sql="$root/tests/compat/postgres-http/algebra-init.sql"
    fixture_init_provenance='client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/PredefinedQueryController.java; tests/compat/postgres-http/{predefined-person.obda,algebra-init.sql,predefined.json,predefined.toml}'
    query='GET /predefined/person?person=https://example.test/person/1 with Accept: text/turtle'
    comparison=http-predefined
    endpoint_predefined=true
    endpoint_health_attempts=15
    postgres_extensions=none
    ;;
  httppredefinedinvalid)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-http/predefined-person.obda"
    mapping_behavior='predefined IRI parameter rejects an invalid IRI with Ontop controller HTTP 500 before query evaluation'
    artifact_case_id=differential-http-predefined-invalid-iri-400
    source_case=none
    init_file=algebra-init.sql
    init_sql="$root/tests/compat/postgres-http/algebra-init.sql"
    fixture_init_provenance='client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/PredefinedQueryController.java; tests/compat/postgres-http/{predefined-person.obda,algebra-init.sql,predefined.json,predefined.toml}'
    query='GET /predefined/person?person=not-an-iri status'
    comparison=http-predefined-invalid
    endpoint_predefined=true
    endpoint_health_attempts=15
    postgres_extensions=none
    ;;
  httpprotocol)
    mapping_file=''
    mapping_source="$baseline/client/cli/src/test/resources/test/simplemapping.obda"
    mapping_behavior='SPARQL endpoint GET, form POST, and application/sparql-query POST protocol'
    artifact_case_id=differential-http-sparql-protocol-forms
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-cli-materialize/init.sql"
    fixture_init_provenance='client/endpoint HTTP protocol; client/cli/src/test/resources/test/simplemapping.obda; tests/compat/postgres-cli-materialize/init.sql'
    query='GET/form POST/application-sparql-query POST ASK'
    comparison=http-protocol
    postgres_extensions=none
    ;;
  httpformats)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-http/mapping-sql-seam.obda"
    mapping_behavior='PostgreSQL endpoint negotiates SPARQL JSON/XML/CSV/TSV, N-Triples CONSTRUCT, and rejects an unsupported Accept value'
    artifact_case_id=differential-postgres-http-result-formats-endpoint
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-query-kinds/init.sql"
    fixture_init_provenance='client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/SparqlQueryController.java; tests/compat/{postgres-query-kinds/init.sql,postgres-http/mapping-sql-seam.obda,postgres-http/sql-seam.rq}'
    query='SELECT person/name and CONSTRUCT person/name with JSON/XML/CSV/TSV/N-Triples Accept negotiation'
    comparison=http-result-formats
    postgres_extensions=none
    ;;
  httpconcurrencyisolation)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-concurrency/mapping-isolation.obda"
    mapping_behavior='PostgreSQL pg_sleep mapped request does not block a concurrent successful SPARQL ASK on the same endpoint'
    artifact_case_id=differential-postgres-http-concurrency-isolation-endpoint
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-query-kinds/init.sql"
    fixture_init_provenance='test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/QuestParallelScenario.java; tests/compat/{postgres-concurrency/mapping-isolation.obda,postgres-concurrency/delayed.rq,postgres-query-kinds/init.sql}'
    query='concurrent pg_sleep ASK and Person ASK'
    comparison=http-concurrency-isolation
    postgres_extensions=none
    ;;
  httpsqlseam)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-http/mapping-sql-seam.obda"
    mapping_behavior='native OBDA two-triple shared-variable BGP with projection and LIMIT over PostgreSQL endpoint'
    artifact_case_id=differential-postgres-sql-reformulated-bgp-endpoint
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-query-kinds/init.sql"
    fixture_init_provenance='client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/SparqlQueryController.java; tests/compat/{postgres-query-kinds/init.sql,postgres-http/mapping-sql-seam.obda,postgres-http/sql-seam.rq}'
    query="$root/tests/compat/postgres-http/sql-seam.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpreformulate)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-http/mapping-sql-seam.obda"
    mapping_behavior='development reformulate endpoint returns PostgreSQL SQL diagnostics and a query correlation ID for default and native-consumption paths'
    artifact_case_id=differential-http-development-reformulate-query-id
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-query-kinds/init.sql"
    fixture_init_provenance='client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/ReformulateController.java; tests/compat/{postgres-query-kinds/init.sql,postgres-http/mapping-sql-seam.obda,postgres-http/reformulate-single-triple.rq}'
    query="$root/tests/compat/postgres-http/reformulate-single-triple.rq"
    comparison=http-reformulate
    ontop_endpoint_options='--dev'
    rtop_development=true
    postgres_extensions=none
    ;;
  httpbag)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-http/mapping-algebra-bag.obda"
    mapping_behavior='PostgreSQL endpoint preserves VALUES multiplicity, OPTIONAL unbound values, MINUS anti-join, and UNION branch multiplicity'
    artifact_case_id=differential-postgres-relational-pattern-bag-endpoint
    source_case=none
    init_file=algebra-init.sql
    init_sql="$root/tests/compat/postgres-http/algebra-init.sql"
    fixture_init_provenance='client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/SparqlQueryController.java; tests/compat/postgres-http/{algebra-init.sql,mapping-algebra-bag.obda,algebra-bag.rq}'
    query="$root/tests/compat/postgres-http/algebra-bag.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpexpressions)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-http/mapping-algebra-bag.obda"
    mapping_behavior='PostgreSQL endpoint preserves IN error short-circuit, OPTIONAL/BOUND, BIND string functions, and NOT IN filtering'
    artifact_case_id=differential-postgres-expression-in-bind-error-endpoint
    source_case=none
    init_file=algebra-init.sql
    init_sql="$root/tests/compat/postgres-http/algebra-init.sql"
    fixture_init_provenance='test/sparql-compliance functions IN/NOT IN; tests/compat/postgres-http/{algebra-init.sql,mapping-algebra-bag.obda,expression-in-bind.rq}'
    query="$root/tests/compat/postgres-http/expression-in-bind.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpdecimalscalar)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-http/mapping-algebra-bag.obda"
    mapping_behavior='PostgreSQL decimal arithmetic and positive/negative scalar ROUND endpoint results'
    artifact_case_id=differential-postgres-decimal-round-scalar-endpoint
    source_case=none
    init_file=algebra-init.sql
    init_sql="$root/tests/compat/postgres-http/algebra-init.sql"
    fixture_init_provenance='tests/compat/postgres-http/{algebra-init.sql,mapping-algebra-bag.obda,decimal-round.rq}'
    query="$root/tests/compat/postgres-http/decimal-round.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpdecimalaggregate)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-http/mapping-algebra-bag.obda"
    mapping_behavior='PostgreSQL decimal aggregate, ROUND(SUM), and SUM(ROUND) endpoint results'
    artifact_case_id=differential-postgres-decimal-round-aggregate-endpoint
    source_case=none
    init_file=algebra-init.sql
    init_sql="$root/tests/compat/postgres-http/algebra-init.sql"
    fixture_init_provenance='tests/compat/postgres-http/{algebra-init.sql,mapping-algebra-bag.obda,decimal-round-aggregate.rq}'
    query="$root/tests/compat/postgres-http/decimal-round-aggregate.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpaggregate)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-http/mapping-algebra-bag.obda"
    mapping_behavior='PostgreSQL aggregate subquery HAVING and expression ORDER BY endpoint results'
    artifact_case_id=differential-postgres-aggregate-having-expression-order-endpoint
    source_case=none
    init_file=algebra-init.sql
    init_sql="$root/tests/compat/postgres-http/algebra-init.sql"
    fixture_init_provenance='SPARQL 1.1 aggregate/subquery/sort assets; tests/compat/postgres-http/{algebra-init.sql,mapping-algebra-bag.obda,aggregate-having-order.rq}'
    query="$root/tests/compat/postgres-http/aggregate-having-order.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpdatasetdefault)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-http/mapping-algebra-bag.obda"
    mapping_behavior='PostgreSQL named graph mapping is visible through FROM as the default dataset graph'
    artifact_case_id=differential-postgres-dataset-from-named-graph-endpoint
    source_case=none
    init_file=algebra-init.sql
    init_sql="$root/tests/compat/postgres-http/algebra-init.sql"
    fixture_init_provenance='SPARQL dataset graph assets; tests/compat/postgres-http/{algebra-init.sql,mapping-algebra-bag.obda,dataset-graph.rq}'
    query="$root/tests/compat/postgres-http/dataset-graph.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpdatasetnamed)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-http/mapping-algebra-bag.obda"
    mapping_behavior='PostgreSQL named graph mapping is visible only through FROM NAMED plus GRAPH'
    artifact_case_id=differential-postgres-dataset-from-named-graph-pattern-endpoint
    source_case=none
    init_file=algebra-init.sql
    init_sql="$root/tests/compat/postgres-http/algebra-init.sql"
    fixture_init_provenance='SPARQL dataset graph assets; tests/compat/postgres-http/{algebra-init.sql,mapping-algebra-bag.obda,dataset-named-graph.rq}'
    query="$root/tests/compat/postgres-http/dataset-named-graph.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpservice)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-http/mapping-algebra-bag.obda"
    mapping_behavior='SERVICE endpoint query is rejected without federation network access'
    artifact_case_id=differential-postgres-service-unsupported-endpoint
    source_case=none
    init_file=algebra-init.sql
    init_sql="$root/tests/compat/postgres-http/algebra-init.sql"
    fixture_init_provenance='SPARQL 1.1 SERVICE assets; tests/compat/postgres-http/algebra fixture'
    query='SELECT ?person { SERVICE <http://example.invalid/sparql> { ?person ?p ?o } }'
    comparison=http-service-error
    postgres_extensions=none
    ;;
  httpregexaddress|httpregexperson)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/pgsql/regex/stockexchangeRegex.obda"
    mapping_behavior='PostgreSQL source SQL case-insensitive regex and negated regex preserve RDF bindings through the endpoint'
    artifact_case_id="differential-postgres-$case_name-endpoint"
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-expressions/init.sql"
    fixture_init_provenance='test/docker-tests/src/test/{java/it/unibz/inf/ontop/docker/postgres/RegexPostgresSQLTest.java,resources/pgsql/regex/stockexchangeRegex.obda}; tests/compat/postgres-expressions/{init.sql,regex-address.rq,regex-person.rq}'
    if [ "$case_name" = httpregexaddress ]; then
      query="$root/tests/compat/postgres-expressions/regex-address.rq"
    else
      query="$root/tests/compat/postgres-expressions/regex-person.rq"
    fi
    comparison=http-json
    postgres_extensions=none
    ;;
  httpidentifierfolding)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/pgsql/identifiers/identifiers-lowercase-postgres.obda"
    mapping_behavior='PostgreSQL unquoted uppercase relation and column identifiers fold to the lowercase catalog while preserving RDF IRI terms'
    artifact_case_id=differential-postgres-unquoted-identifier-folding-endpoint
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-datatype-manifest/init.sql"
    fixture_init_provenance='test/docker-tests/src/test/resources/pgsql/identifiers/identifiers-lowercase-postgres.obda; tests/compat/postgres-datatype-manifest/init.sql; PostgresIdentifierTest PostgreSQL fixture'
    query="$root/tests/compat/postgres-identifiers/country.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpquotedalias)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/pgsql/identifiers/identifiers-postgres.obda"
    mapping_behavior='PostgreSQL quoted AS "LETTER" projection alias is preserved when the mapping template refers to {LETTER}'
    artifact_case_id=differential-postgres-quoted-identifier-alias-endpoint
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-datatype-manifest/init.sql"
    fixture_init_provenance='test/docker-tests/src/test/resources/pgsql/identifiers/identifiers-postgres.obda; tests/compat/postgres-datatype-manifest/init.sql; PostgresIdentifierTest PostgreSQL fixture'
    query="$root/tests/compat/postgres-identifiers/country3.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpcastscalar|httpcastinvalid|httpcastmappeddate)
    mapping_file=''
    mapping_source="$baseline/test/lightweight-tests/src/test/resources/books/books.obda"
    mapping_behavior='PostgreSQL CastPostgreSQL scalar lexical result and invalid cast expression-error bindings through the endpoint'
    artifact_case_id="differential-postgres-$case_name-endpoint"
    source_case=none
    init_file=init.sql
    if [ "$case_name" = httpcastmappeddate ]; then
      init_sql="$root/tests/compat/postgres-cast/init-mapped-datetime.sql"
      fixture_init_provenance='test/lightweight-tests/src/test/{java/it/unibz/inf/ontop/docker/lightweight/postgresql/CastPostgreSQLTest.java,resources/books/books.obda}; tests/compat/postgres-cast/{init-mapped-datetime.sql,cast-date-from-mapped-datetime.rq}'
      query="$root/tests/compat/postgres-cast/cast-date-from-mapped-datetime.rq"
    else
      init_sql="$root/tests/compat/postgres-expressions/init.sql"
      fixture_init_provenance='test/lightweight-tests/src/test/{java/it/unibz/inf/ontop/docker/lightweight/postgresql/CastPostgreSQLTest.java,resources/books/books.obda}; tests/compat/postgres-expressions/init.sql; tests/compat/postgres-cast queries'
      if [ "$case_name" = httpcastscalar ]; then
        query="$root/tests/compat/postgres-cast/cast-float-from-double.rq"
      else
        query="$root/tests/compat/postgres-cast/cast-invalid-direct.rq"
      fi
    fi
    comparison=http-json
    postgres_extensions=none
    ;;
  httplowermovie)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/pgsql/lowerMovie.obda"
    mapping_behavior='PostgreSQL lower() native source SQL preserves the MovieOntology title RDF literal through the endpoint'
    artifact_case_id=differential-postgres-native-obda-legal-source-lower-term-endpoint
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-identifiers/lower-movie-init.sql"
    fixture_init_provenance='test/docker-tests/src/test/resources/pgsql/lowerMovie.obda; tests/compat/postgres-identifiers/lower-movie-init.sql; LowerMovieTest PostgreSQL fixture'
    query="$root/tests/compat/postgres-identifiers/lower-title.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpuniversitysubclass)
    mapping_file=''
    mapping_source="$baseline/binding/rdf4j/src/test/resources/tbox-facts/university.obda"
    ontology_source="$root/tests/compat/postgres-ontology/endpoint-imports-university.ttl"
    ontology_import_sources="$baseline/binding/rdf4j/src/test/resources/tbox-facts/university-complete.ttl"
    mapping_behavior='relative OWL imports closure rewrites PostgreSQL Student mapping to foaf:Person through the SPARQL endpoint'
    artifact_case_id=differential-postgres-ontology-imported-subclass-rewrite-endpoint
    source_case=none
    init_file=university.sql
    init_sql="$baseline/binding/rdf4j/src/test/resources/tbox-facts/university.sql"
    fixture_init_provenance='binding/rdf4j/src/test/{java/it/unibz/inf/ontop/rdf4j/repository/UniversityTBoxFactTest.java,resources/tbox-facts/{university.sql,university.obda,university-complete.ttl}}; tests/compat/postgres-ontology/{endpoint-imports-university.ttl,subclass-imported.rq}'
    query="$root/tests/compat/postgres-ontology/subclass-imported.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpimportscatalog)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-http/mapping-algebra-bag.obda"
    ontology_source="$root/tests/compat/postgres-http/imports-root.ttl"
    ontology_import_sources="$root/tests/compat/postgres-http/imports-child.ttl"
    ontology_catalog_source="$root/tests/compat/postgres-http/imports-catalog.xml"
    mapping_behavior='XML Catalog redirects top-level ontology import IRIs and closes the root-child cycle for PostgreSQL endpoint rewriting'
    artifact_case_id=differential-postgres-ontology-imports-catalog-endpoint
    source_case=none
    init_file=algebra-init.sql
    init_sql="$root/tests/compat/postgres-http/algebra-init.sql"
    fixture_init_provenance='tests/compat/postgres-http/{imports-catalog.xml,imports-root.ttl,imports-child.ttl,mapping-algebra-bag.obda,algebra-init.sql,imports-catalog.rq}'
    query="$root/tests/compat/postgres-http/imports-catalog.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httptboxequivalentclass|httptboxequivalentproperty|httptboxexistentialdomain|httptboxqualifiedlimit)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-http/mapping-algebra-bag.obda"
    ontology_source="$root/tests/compat/postgres-http/imports-root.ttl"
    ontology_import_sources="$root/tests/compat/postgres-http/imports-child.ttl"
    ontology_catalog_source="$root/tests/compat/postgres-http/imports-catalog.xml"
    mapping_behavior='OWL 2 QL TBox closure and qualified existential limit through XML Catalog imports on PostgreSQL endpoint'
    artifact_case_id="differential-postgres-owl-ql-$case_name-endpoint"
    source_case=none
    init_file=algebra-init.sql
    init_sql="$root/tests/compat/postgres-http/algebra-init.sql"
    fixture_init_provenance='tests/compat/postgres-http/{imports-catalog.xml,imports-root.ttl,imports-child.ttl,mapping-algebra-bag.obda,algebra-init.sql,tbox-*.rq}'
    case "$case_name" in
      httptboxequivalentclass) query="$root/tests/compat/postgres-http/tbox-equivalent-class.rq" ;;
      httptboxequivalentproperty) query="$root/tests/compat/postgres-http/tbox-equivalent-property.rq" ;;
      httptboxexistentialdomain) query="$root/tests/compat/postgres-http/tbox-existential-domain.rq" ;;
      httptboxqualifiedlimit) query="$root/tests/compat/postgres-http/tbox-qualified-existential-limit.rq" ;;
    esac
    comparison=http-json
    postgres_extensions=none
    ;;
  httpuniversitydomainrange)
    mapping_file=''
    mapping_source="$baseline/binding/rdf4j/src/test/resources/tbox-facts/university.obda"
    ontology_source="$root/tests/compat/postgres-ontology/endpoint-imports-university.ttl"
    ontology_import_sources="$baseline/binding/rdf4j/src/test/resources/tbox-facts/university-complete.ttl"
    facts_source="$root/tests/compat/postgres-ontology/facts.ttl"
    mapping_behavior='OWL 2 QL teaches domain and range axioms infer Teacher and Course from endpoint facts'
    artifact_case_id=differential-postgres-ontology-domain-range-facts-endpoint
    source_case=none
    init_file=university.sql
    init_sql="$baseline/binding/rdf4j/src/test/resources/tbox-facts/university.sql"
    fixture_init_provenance='binding/rdf4j/src/test/{java/it/unibz/inf/ontop/rdf4j/repository/UniversityTBoxFactTest.java,resources/tbox-facts/{university.sql,university.obda,university-complete.ttl}}; tests/compat/postgres-ontology/{endpoint-imports-university.ttl,facts.ttl,domain-range-facts.rq}'
    query="$root/tests/compat/postgres-ontology/domain-range-facts.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpontologydisjoint)
    mapping_file=''
    mapping_source="$baseline/binding/rdf4j/src/test/resources/tbox-facts/university.obda"
    ontology_source="$root/tests/compat/postgres-ontology/endpoint-imports-university.ttl"
    ontology_import_sources="$baseline/binding/rdf4j/src/test/resources/tbox-facts/university-complete.ttl"
    facts_source="$root/tests/compat/postgres-ontology/inconsistent-facts.ttl"
    mapping_behavior='University Course and foaf:Person disjoint facts input boundary'
    artifact_case_id=differential-postgres-ontology-disjoint-inconsistency-endpoint
    source_case=none
    init_file=university.sql
    init_sql="$baseline/binding/rdf4j/src/test/resources/tbox-facts/university.sql"
    fixture_init_provenance='binding/rdf4j/src/test/resources/tbox-facts/{university.sql,university.obda,university-complete.ttl}; tests/compat/postgres-ontology/{endpoint-imports-university.ttl,inconsistent-facts.ttl,disjoint-probe.rq}'
    query="$root/tests/compat/postgres-ontology/disjoint-probe.rq"
    comparison=http-json
    endpoint_health_attempts=15
    postgres_extensions=none
    ;;
  httpuniversitysubproperty|httpuniversityinverse)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-ontology/teaching.obda"
    ontology_source="$root/tests/compat/postgres-ontology/endpoint-imports-university.ttl"
    ontology_import_sources="$baseline/binding/rdf4j/src/test/resources/tbox-facts/university-complete.ttl"
    mapping_behavior='OWL 2 QL property closure rewrites givesLab mapping through teaches and isTaughtBy'
    artifact_case_id="differential-postgres-ontology-$case_name-endpoint"
    source_case=none
    init_file=university.sql
    init_sql="$baseline/binding/rdf4j/src/test/resources/tbox-facts/university.sql"
    fixture_init_provenance='binding/rdf4j/src/test/{java/it/unibz/inf/ontop/rdf4j/repository/UniversityTBoxFactTest.java,resources/tbox-facts/{university.sql,university-complete.ttl}}; tests/compat/postgres-ontology/{endpoint-imports-university.ttl,teaching.obda,subproperty.rq,inverse.rq}'
    if [ "$case_name" = httpuniversitysubproperty ]; then
      query="$root/tests/compat/postgres-ontology/subproperty.rq"
    else
      query="$root/tests/compat/postgres-ontology/inverse.rq"
    fi
    comparison=http-json
    postgres_extensions=none
    ;;
  httpprefixsource)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/pgsql/imdb/newPrefixMovieOntology.obda"
    mapping_behavior='native OBDA PrefixDeclaration preserves mo/mo2 namespace expansion inside target RDF IRI terms'
    artifact_case_id=differential-postgres-native-obda-prefix-declaration-iri-term-endpoint
    source_case=none
    init_file=prefix-source-init.sql
    init_sql="$root/tests/compat/postgres-identifiers/prefix-source-init.sql"
    fixture_init_provenance='test/docker-tests/src/test/{java/it/unibz/inf/ontop/docker/postgres/PrefixSourceTest.java,resources/pgsql/imdb/newPrefixMovieOntology.obda}; tests/compat/postgres-identifiers/{prefix-source-init.sql,prefix-source.rq}'
    query="$root/tests/compat/postgres-identifiers/prefix-source.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpepnet)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/pgsql/EPNet.obda"
    mapping_behavior='native OBDA expands {rp_id} dynamic class IRI and {i_id} subject IRI through PostgreSQL EPNet relations'
    artifact_case_id=differential-postgres-epnet-dynamic-meta-mapping-endpoint
    source_case=none
    init_file=epnet-init.sql
    init_sql="$root/tests/compat/postgres-metamapping/epnet-init.sql"
    fixture_init_provenance='test/docker-tests/src/test/{java/it/unibz/inf/ontop/docker/postgres/MetaMappingExpanderTest.java,resources/pgsql/EPNet.obda}; tests/compat/postgres-metamapping/{epnet-init.sql,query.rq}'
    query="$root/tests/compat/postgres-metamapping/query.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpfactsturtle)
    mapping_file=''
    mapping_source="$baseline/binding/rdf4j/src/test/resources/facts/mapping.obda"
    ontology_source="$baseline/binding/rdf4j/src/test/resources/facts/ontology.ttl"
    facts_source="$baseline/binding/rdf4j/src/test/resources/facts/facts.ttl"
    mapping_behavior='Turtle facts and PostgreSQL company mapping form one DISTINCT/ORDER BY virtual graph result'
    artifact_case_id=differential-postgres-facts-turtle-mapping-union-endpoint
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-facts/init.sql"
    fixture_init_provenance='binding/rdf4j/src/test/{java/it/unibz/inf/ontop/rdf4j/repository/FactsFileTest{,Turtle}.java,resources/facts/{mapping.obda,ontology.ttl,facts.ttl}}; tests/compat/postgres-facts/{init.sql,companies.rq}'
    query="$root/tests/compat/postgres-facts/companies.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpfactsnquads)
    mapping_file=''
    mapping_source="$baseline/binding/rdf4j/src/test/resources/facts/mapping.obda"
    ontology_source="$baseline/binding/rdf4j/src/test/resources/facts/ontology.ttl"
    facts_source="$baseline/binding/rdf4j/src/test/resources/facts/facts.nq"
    facts_format=nquads
    mapping_behavior='N-Quads facts preserve the named graph and typed integer binding alongside PostgreSQL mapping input'
    artifact_case_id=differential-postgres-facts-nquads-named-graph-endpoint
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-facts/init.sql"
    fixture_init_provenance='binding/rdf4j/src/test/{java/it/unibz/inf/ontop/rdf4j/repository/FactsFileTest{,NQuads}.java,resources/facts/{mapping.obda,ontology.ttl,facts.nq}}; tests/compat/postgres-facts/{init.sql,named-graph.rq}'
    query="$root/tests/compat/postgres-facts/named-graph.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpfactsrdfxml)
    mapping_file=''
    mapping_source="$baseline/binding/rdf4j/src/test/resources/facts/mapping.obda"
    ontology_source="$baseline/binding/rdf4j/src/test/resources/facts/ontology.ttl"
    facts_source="$baseline/binding/rdf4j/src/test/resources/facts/facts.rdf"
    facts_format=rdfxml
    facts_base_iri=http://ontop-vkg.org/facts#
    mapping_behavior='RDF/XML facts with the explicit facts base preserve the Company integer binding alongside PostgreSQL mapping input'
    artifact_case_id=differential-postgres-facts-rdfxml-explicit-base-endpoint
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-facts/init.sql"
    fixture_init_provenance='binding/rdf4j/src/test/{java/it/unibz/inf/ontop/rdf4j/repository/FactsFileTest{,RDF}.java,resources/facts/{mapping.obda,ontology.ttl,facts.rdf}}; tests/compat/postgres-facts/{init.sql,rdfxml-company.rq}'
    query="$root/tests/compat/postgres-facts/rdfxml-company.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpfactsmalformed)
    mapping_file=''
    mapping_source="$baseline/binding/rdf4j/src/test/resources/facts/mapping.obda"
    facts_source="$root/tests/compat/postgres-facts/malformed.ttl"
    mapping_behavior='Malformed Turtle facts are rejected during endpoint initialization'
    artifact_case_id=differential-postgres-facts-malformed-turtle-endpoint
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-facts/init.sql"
    fixture_init_provenance='binding/rdf4j/src/test/resources/facts/mapping.obda; tests/compat/postgres-facts/{init.sql,malformed.ttl}'
    query='endpoint initialization with malformed Turtle facts'
    comparison=http-json
    postgres_extensions=none
    ;;
  httpfactsmissing)
    mapping_file=''
    mapping_source="$baseline/binding/rdf4j/src/test/resources/facts/mapping.obda"
    facts_source="$root/tests/compat/postgres-facts/missing.ttl"
    facts_allow_missing=true
    mapping_behavior='Missing Turtle facts are rejected during endpoint initialization'
    artifact_case_id=differential-postgres-facts-missing-turtle-endpoint
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-facts/init.sql"
    fixture_init_provenance='binding/rdf4j/src/test/resources/facts/mapping.obda; tests/compat/postgres-facts/missing.ttl (intentionally absent)'
    query='endpoint initialization with missing Turtle facts'
    comparison=http-json
    postgres_extensions=none
    ;;
  httpnativeinvalidsql)
    mapping_file=''
    mapping_source="$baseline/mapping/sql/all/src/test/resources/mistake/invalid-sql1.obda"
    mapping_behavior='Invalid native source SQL permits endpoint startup and fails when its mapped source is first queried'
    artifact_case_id=differential-postgres-native-obda-invalid-source-sql-endpoint
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-minimal/init.sql"
    fixture_init_provenance='mapping/sql/all/src/test/resources/mistake/invalid-sql1.obda; tests/compat/postgres-minimal/init.sql'
    query="$root/tests/compat/postgres-native-obda/runtime-source.rq"
    comparison=http-native-source-sql-error
    postgres_extensions=none
    # 这个 fixture 的每一个涉及 mapping 的 SPARQL 查询都会失败；不能用 ASK
    # 作为 listener readiness probe。固定 Ontop endpoint 的 actuator health 不会
    # 触发 query reformulation，恰好对应此原子的启动成功边界。
    endpoint_health=actuator
    ;;
  httpnativetermsnull)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-http/mapping-algebra-bag.obda"
    mapping_behavior='Native OBDA preserves language RDF terms and omits NULL source values through PostgreSQL endpoint'
    artifact_case_id=differential-postgres-native-obda-terms-null-endpoint
    source_case=none
    init_file=algebra-init.sql
    init_sql="$root/tests/compat/postgres-http/algebra-init.sql"
    fixture_init_provenance='tests/compat/postgres-http/{mapping-algebra-bag.obda,algebra-init.sql,native-obda-terms-null.rq}'
    query="$root/tests/compat/postgres-http/native-obda-terms-null.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  nativemissingtarget)
    mapping_file=''
    mapping_source="$baseline/mapping/sql/all/src/test/resources/mistake/missing-target-term.obda"
    mapping_behavior='Missing native OBDA target term is rejected by the mapping reader before PostgreSQL source execution'
    artifact_case_id=differential-postgres-native-obda-reader-missing-target
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-minimal/init.sql"
    fixture_init_provenance='mapping/sql/all/src/test/resources/mistake/missing-target-term.obda; tests/compat/postgres-minimal/init.sql'
    query='CLI materialize mapping-reader rejection'
    comparison=native-reader-rejection
    postgres_extensions=none
    ;;
  nativeruntimemissingrelation)
    mapping_file=''
    mapping_source="$baseline/mapping/sql/all/src/test/resources/mistake/correct.obda"
    mapping_behavior='Valid native OBDA source fails at PostgreSQL execution when PERSON relation is absent'
    artifact_case_id=differential-postgres-native-obda-runtime-source-relation-error
    source_case=none
    init_file=empty-schema.sql
    init_sql="$root/tests/compat/postgres-native-obda/empty-schema.sql"
    fixture_init_provenance='mapping/sql/all/src/test/resources/mistake/correct.obda; tests/compat/postgres-native-obda/empty-schema.sql (PERSON intentionally absent)'
    query='CLI materialize valid mapping against absent PERSON relation'
    comparison=native-source-relation-error
    postgres_extensions=none
    ;;
  httpnestedjson|httpnestedjsonb|httpnestedarray)
    mapping_file=''
    mapping_behavior='PostgreSQL nested JSON/JSONB/array source SQL preserves aggregate RDF lexical bindings through the endpoint'
    artifact_case_id="differential-postgres-$case_name-endpoint"
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-nested/init.sql"
    fixture_init_provenance='test/lightweight-tests/src/test/{java/it/unibz/inf/ontop/docker/lightweight/postgresql/NestedData{JSON,JSONB,Array}PostgreSQLTest.java,resources/nested/nested.obda}; tests/compat/postgres-nested/{init.sql,nested-json.obda,nested-jsonb.obda,nested-array.obda,flatten-aggregate.rq}'
    case "$case_name" in
      httpnestedjson)
        mapping_source="$root/tests/compat/postgres-nested/nested-json.obda"
        ontop_mapping_source="$baseline/test/lightweight-tests/src/test/resources/nested/nested.obda"
        ontop_lenses_source="$baseline/test/lightweight-tests/src/test/resources/nested/postgresql/nested-lenses-json.json"
        ;;
      httpnestedjsonb)
        mapping_source="$root/tests/compat/postgres-nested/nested-jsonb.obda"
        ontop_mapping_source="$baseline/test/lightweight-tests/src/test/resources/nested/nested.obda"
        ontop_lenses_source="$baseline/test/lightweight-tests/src/test/resources/nested/postgresql/nested-lenses-jsonb.json"
        ;;
      httpnestedarray)
        mapping_source="$root/tests/compat/postgres-nested/nested-array.obda"
        ontop_mapping_source="$baseline/test/lightweight-tests/src/test/resources/nested/nested.obda"
        ontop_lenses_source="$baseline/test/lightweight-tests/src/test/resources/nested/postgresql/nested-lenses-array.json"
        ;;
    esac
    query="$root/tests/compat/postgres-nested/flatten-aggregate.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httppostgisintersection1|httppostgisintersection2|httppostgisintersection3)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-geospatial/geospatial.obda"
    ontop_mapping_source="$baseline/test/lightweight-tests/src/test/resources/geospatial/geospatial.obda"
    mapping_behavior='GeoSPARQL PostGIS geometry/geography buffer intersection preserves RDF WKT terms and mixed-type empty results through the endpoint'
    artifact_case_id="differential-postgres-$case_name-endpoint"
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-geospatial/init.sql"
    fixture_init_provenance='test/lightweight-tests/src/test/{java/it/unibz/inf/ontop/docker/lightweight/postgresql/other/GeoSPARQLPostGISTest.java,resources/geospatial/geospatial.obda}; tests/compat/postgres-geospatial/{init.sql,geospatial.obda,intersection-1.rq,intersection-2.rq,intersection-3.rq}'
    case "$case_name" in
      httppostgisintersection1) query="$root/tests/compat/postgres-geospatial/intersection-1.rq" ;;
      httppostgisintersection2) query="$root/tests/compat/postgres-geospatial/intersection-2.rq" ;;
      httppostgisintersection3) query="$root/tests/compat/postgres-geospatial/intersection-3.rq" ;;
    esac
    comparison=http-json
    postgres_extensions=postgis
    required_postgres_digest=sha256:01a6a70e41e6c4467c8f55f6063555ed72db2d6662cd0d571040d42eadaeb6f6
    ;;
  httpprofoptional)
    mapping_file=''
    mapping_source="$baseline/test/lightweight-tests/src/test/resources/prof/prof.obda"
    ontology_source="$baseline/test/lightweight-tests/src/test/resources/prof/prof.owl"
    mapping_behavior='ConstraintPostgreSQL professors primary key and nullable nickname preserve OPTIONAL unbound bindings through the endpoint'
    artifact_case_id=differential-postgres-prof-optional-nickname-endpoint
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-aggregates/init.sql"
    fixture_init_provenance='test/lightweight-tests/src/test/{java/it/unibz/inf/ontop/docker/lightweight/{AbstractConstraintTest,postgresql/ConstraintPostgreSQLTest}.java,resources/prof/{prof.obda,prof.owl}}; tests/compat/postgres-aggregates/{init.sql,optional-simple-nickname.rq}'
    query="$root/tests/compat/postgres-aggregates/optional-simple-nickname.rq"
    ontop_infer_default_datatype=true
    comparison=http-json
    postgres_extensions=none
    ;;
  httpproffkjoin|httpprofaggregate)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/redundant_join/redundant_join_fk_test.obda"
    ontology_source="$baseline/test/docker-tests/src/test/resources/redundant_join/redundant_join_fk_test.owl"
    mapping_behavior='PostgreSQL course/professors/teaching primary and foreign keys preserve LEFT JOIN bag and aggregate mapping RDF terms through the endpoint'
    artifact_case_id="differential-postgres-$case_name-endpoint"
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-aggregates/init.sql"
    fixture_init_provenance='test/docker-tests/src/test/{java/it/unibz/inf/ontop/docker/{AbstractLeftJoinProfTest,postgres/LeftJoinProfPgSQLTest}.java,resources/redundant_join/{redundant_join_fk_test.obda,redundant_join_fk_test.owl}}; tests/compat/postgres-aggregates/{init.sql,course-join-left-2.rq,aggregation-mapping-prof-student-count-property.rq}'
    if [ "$case_name" = httpproffkjoin ]; then
      query="$root/tests/compat/postgres-aggregates/course-join-left-2.rq"
    else
      query="$root/tests/compat/postgres-aggregates/aggregation-mapping-prof-student-count-property.rq"
    fi
    ontop_infer_default_datatype=true
    rtop_mapping_infer_default_datatype=false
    comparison=http-json
    postgres_extensions=none
    ;;
  httpdatatypeboolean)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/boolean/datatypes-pgsql.obda"
    mapping_behavior='PgsqlDatatypeTest boolean manifest query preserves xsd:boolean binding and lexical filter semantics through the endpoint'
    artifact_case_id=differential-postgres-datatype-manifest-boolean-boolean
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-datatype-manifest/init.sql"
    fixture_init_provenance='test/docker-tests/src/test/{java/it/unibz/inf/ontop/docker/datatypes/PgsqlDatatypeTest.java,resources/testcases-docker/boolean/{datatypes-pgsql.obda,boolean.rq}}; tests/compat/postgres-datatype-manifest/init.sql'
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/boolean/boolean.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpdatatypecharacterchar)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/datatypes-pgsql.obda"
    mapping_behavior='PgsqlDatatypeTest character char manifest query preserves simple-literal binding and lexical filter semantics through the endpoint'
    artifact_case_id=differential-postgres-datatype-manifest-character-char
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-datatype-manifest/init.sql"
    fixture_init_provenance='test/docker-tests/src/test/{java/it/unibz/inf/ontop/docker/datatypes/PgsqlDatatypeTest.java,resources/testcases-docker/character/{datatypes-pgsql.obda,char.rq}}; tests/compat/postgres-datatype-manifest/init.sql'
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/char.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpdatatypecharacterchargraph)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/datatypes-pgsql.obda"
    mapping_behavior='PgsqlDatatypeTest character char graph-named manifest query preserves simple-literal graph query semantics through the endpoint'
    artifact_case_id=differential-postgres-datatype-manifest-character-char-graph
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-datatype-manifest/init.sql"
    fixture_init_provenance='test/docker-tests/src/test/{java/it/unibz/inf/ontop/docker/datatypes/PgsqlDatatypeTest.java,resources/testcases-docker/character/{datatypes-pgsql.obda,char-graph.rq}}; tests/compat/postgres-datatype-manifest/init.sql'
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/char-graph.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpdatatypecharactervarchar)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/datatypes-pgsql.obda"
    mapping_behavior='PgsqlDatatypeTest character varchar manifest query preserves simple-literal binding and lexical filter semantics through the endpoint'
    artifact_case_id=differential-postgres-datatype-manifest-character-varchar
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-datatype-manifest/init.sql"
    fixture_init_provenance='test/docker-tests/src/test/{java/it/unibz/inf/ontop/docker/datatypes/PgsqlDatatypeTest.java,resources/testcases-docker/character/{datatypes-pgsql.obda,varchar.rq}}; tests/compat/postgres-datatype-manifest/init.sql'
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/varchar.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpdatatypecharactervarchargraph)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/datatypes-pgsql.obda"
    mapping_behavior='PgsqlDatatypeTest character varchar graph-named manifest query preserves simple-literal graph query semantics through the endpoint'
    artifact_case_id=differential-postgres-datatype-manifest-character-varchar-graph
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-datatype-manifest/init.sql"
    fixture_init_provenance='test/docker-tests/src/test/{java/it/unibz/inf/ontop/docker/datatypes/PgsqlDatatypeTest.java,resources/testcases-docker/character/{datatypes-pgsql.obda,varchar-graph.rq}}; tests/compat/postgres-datatype-manifest/init.sql'
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/varchar-graph.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpdatatypecharactertext)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/datatypes-pgsql.obda"
    mapping_behavior='PgsqlDatatypeTest character text manifest query preserves simple-literal binding and lexical filter semantics through the endpoint'
    artifact_case_id=differential-postgres-datatype-manifest-character-text
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-datatype-manifest/init.sql"
    fixture_init_provenance='test/docker-tests/src/test/{java/it/unibz/inf/ontop/docker/datatypes/PgsqlDatatypeTest.java,resources/testcases-docker/character/{datatypes-pgsql.obda,text.rq}}; tests/compat/postgres-datatype-manifest/init.sql'
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/text.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpdatatypecharactertextgraph)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/datatypes-pgsql.obda"
    mapping_behavior='PgsqlDatatypeTest character text graph-named manifest query preserves simple-literal graph query semantics through the endpoint'
    artifact_case_id=differential-postgres-datatype-manifest-character-text-graph
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-datatype-manifest/init.sql"
    fixture_init_provenance='test/docker-tests/src/test/{java/it/unibz/inf/ontop/docker/datatypes/PgsqlDatatypeTest.java,resources/testcases-docker/character/{datatypes-pgsql.obda,text-graph.rq}}; tests/compat/postgres-datatype-manifest/init.sql'
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/text-graph.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpdatatypecharactercharacter)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/datatypes-pgsql.obda"
    mapping_behavior='PgsqlDatatypeTest character character manifest query preserves simple-literal binding and lexical filter semantics through the endpoint'
    artifact_case_id=differential-postgres-datatype-manifest-character-character
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-datatype-manifest/init.sql"
    fixture_init_provenance='test/docker-tests/src/test/{java/it/unibz/inf/ontop/docker/datatypes/PgsqlDatatypeTest.java,resources/testcases-docker/character/{datatypes-pgsql.obda,character.rq}}; tests/compat/postgres-datatype-manifest/init.sql'
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/character.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpdatatypecharactercharactergraph)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/datatypes-pgsql.obda"
    mapping_behavior='PgsqlDatatypeTest character character graph-named manifest query preserves simple-literal graph query semantics through the endpoint'
    artifact_case_id=differential-postgres-datatype-manifest-character-character-graph
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-datatype-manifest/init.sql"
    fixture_init_provenance='test/docker-tests/src/test/{java/it/unibz/inf/ontop/docker/datatypes/PgsqlDatatypeTest.java,resources/testcases-docker/character/{datatypes-pgsql.obda,character-graph.rq}}; tests/compat/postgres-datatype-manifest/init.sql'
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/character-graph.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpdatatypecharactername)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/datatypes-pgsql.obda"
    mapping_behavior='PgsqlDatatypeTest character name manifest query preserves PostgreSQL name simple-literal binding and lexical filter semantics through the endpoint'
    artifact_case_id=differential-postgres-datatype-manifest-character-name
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-datatype-manifest/init.sql"
    fixture_init_provenance='test/docker-tests/src/test/{java/it/unibz/inf/ontop/docker/datatypes/PgsqlDatatypeTest.java,resources/testcases-docker/character/{datatypes-pgsql.obda,name.rq}}; tests/compat/postgres-datatype-manifest/init.sql'
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/name.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpdatatypecharacternamegraph)
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/datatypes-pgsql.obda"
    mapping_behavior='PgsqlDatatypeTest character name graph-named manifest query preserves PostgreSQL name simple-literal graph query semantics through the endpoint'
    artifact_case_id=differential-postgres-datatype-manifest-character-name-graph
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-datatype-manifest/init.sql"
    fixture_init_provenance='test/docker-tests/src/test/{java/it/unibz/inf/ontop/docker/datatypes/PgsqlDatatypeTest.java,resources/testcases-docker/character/{datatypes-pgsql.obda,name-graph.rq}}; tests/compat/postgres-datatype-manifest/init.sql'
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/character/name-graph.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httpdatatypemanifest)
    datatype_group=${DATATYPE_MANIFEST_GROUP:-}
    datatype_query=${DATATYPE_MANIFEST_QUERY:-}
    case "$datatype_group:$datatype_query" in
      datetime:dateLiteral|datetime:date|datetime:date-str|datetime:date-bgp|datetime:timeLiteral|datetime:time|datetime:time-str|datetime:time-bgp|datetime:time_tz_Literal|datetime:time_tz|datetime:time_tz-bgp|datetime:timestamp|datetime:timestamp-str|datetime:timestamp_tz|datetime:timestamp_tz-str|numeric:integer|numeric:smallint|numeric:bigint|numeric:numeric|numeric:real|numeric:double|numeric:serial|numeric:bigserial) ;;
      *) printf '%s\n' "未知 DATATYPE_MANIFEST_GROUP/QUERY：$datatype_group/$datatype_query" >&2; exit 64 ;;
    esac
    mapping_file=''
    mapping_source="$baseline/test/docker-tests/src/test/resources/testcases-docker/$datatype_group/datatypes-pgsql.obda"
    mapping_behavior="PgsqlDatatypeTest $datatype_group/$datatype_query 原始 manifest query 的 PostgreSQL typed binding"
    artifact_case_id="differential-postgres-datatype-manifest-$datatype_group-$datatype_query"
    source_case=none
    init_file=init.sql
    init_sql="$root/tests/compat/postgres-datatype-manifest/init.sql"
    fixture_init_provenance="test/docker-tests/src/test/{java/it/unibz/inf/ontop/docker/datatypes/PgsqlDatatypeTest.java,resources/testcases-docker/$datatype_group/{datatypes-pgsql.obda,$datatype_query.rq}}; tests/compat/postgres-datatype-manifest/init.sql"
    query="$baseline/test/docker-tests/src/test/resources/testcases-docker/$datatype_group/$datatype_query.rq"
    comparison=http-json
    postgres_extensions=none
    ;;
  httppathexists|httppathnotexists)
    mapping_file=''
    mapping_source="$root/tests/compat/postgres-http/mapping-algebra-bag.obda"
    mapping_behavior='PostgreSQL property path and correlated EXISTS/NOT EXISTS endpoint results'
    artifact_case_id="differential-postgres-$case_name-endpoint"
    source_case=none
    init_file=algebra-init.sql
    init_sql="$root/tests/compat/postgres-http/algebra-init.sql"
    fixture_init_provenance='SPARQL 1.1 property path/EXISTS assets; tests/compat/postgres-http algebra fixture'
    if [ "$case_name" = httppathexists ]; then
      query="$root/tests/compat/postgres-http/property-path-exists.rq"
      comparison=http-json
    else
      query="$root/tests/compat/postgres-http/not-exists-correlation.rq"
      comparison=http-error
    fi
    postgres_extensions=none
    ;;
  dmd000)
    mapping_file=''
    mapping_behavior='Direct Mapping empty PostgreSQL relation'
    artifact_case_id=differential-direct-mapping-d000-empty-relation
    source_case=D000
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D000/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D000/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=pgcrypto
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations='Student'
    ;;
  dmd001)
    mapping_file=''
    mapping_behavior='Direct Mapping no-primary-key single-column relation'
    artifact_case_id=differential-direct-mapping-d001-no-primary-key
    source_case=D001
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D001/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D001/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=pgcrypto
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations='Student'
    ;;
  dmd002)
    mapping_file=''
    mapping_behavior='Direct Mapping no-primary-key two-column blank-node relation'
    artifact_case_id=differential-direct-mapping-d002-two-columns-no-primary-key
    source_case=D002
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D002/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D002/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=pgcrypto
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations='Student'
    ;;
  dmd003)
    mapping_file=''
    mapping_behavior='Direct Mapping no-primary-key three-column blank-node relation'
    artifact_case_id=differential-direct-mapping-d003-three-columns-no-primary-key
    source_case=D003
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D003/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D003/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=pgcrypto
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations='Student'
    ;;
  dmd004)
    mapping_file=''
    mapping_behavior='Direct Mapping no-primary-key two-column blank-node relation'
    artifact_case_id=differential-direct-mapping-d004-no-primary-key-two-columns
    source_case=D004
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D004/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D004/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=pgcrypto
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations='Student_Sport'
    ;;
  dmd005)
    mapping_file=''
    mapping_behavior='Direct Mapping duplicate no-primary-key blank nodes and xsd:double values'
    artifact_case_id=differential-direct-mapping-d005-duplicate-blank-nodes-and-double
    source_case=D005
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D005/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D005/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=pgcrypto
    direct_mapping_preserve_physical_rows=false
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations='IOUs'
    ;;
  dmd011)
    mapping_file=''
    mapping_behavior='Direct Mapping many-to-many composite primary-key link table and two foreign keys'
    artifact_case_id=differential-direct-mapping-d011-many-to-many-composite-link-table
    source_case=D011
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D011/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D011/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=none
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations_toml='["Student", "Sport", "Student_Sport"]'
    ;;
  dmd012)
    mapping_file=''
    mapping_behavior='Direct Mapping two no-primary-key relations with duplicate blank-node rows'
    artifact_case_id=differential-direct-mapping-d012-two-no-primary-key-relations
    source_case=D012
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D012/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D012/{create.sql,directGraph.ttl,directGraph-modified.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=pgcrypto
    direct_mapping_preserve_physical_rows=false
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations_toml='["IOUs", "Lives"]'
    ;;
  dmd014)
    mapping_file=''
    mapping_behavior='Direct Mapping foreign key to no-primary-key parent blank node'
    artifact_case_id=differential-direct-mapping-d014-foreign-key-to-no-primary-key-parent
    source_case=D014
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D014/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D014/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=pgcrypto
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations_toml='["DEPT", "EMP", "LIKES"]'
    ;;
  dmd018)
    mapping_file=''
    mapping_behavior='Direct Mapping no-primary-key CHAR(15) fixed-width literals'
    artifact_case_id=differential-direct-mapping-d018-char-fixed-width-literals
    source_case=D018
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D018/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D018/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=pgcrypto
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations='Student'
    ;;
  dmd016)
    mapping_file=''
    mapping_behavior='Direct Mapping PostgreSQL SQL datatypes and bytea binary conversion'
    artifact_case_id=differential-direct-mapping-d016-postgresql-sql-datatypes-and-binary
    source_case=D016
    init_file=postgres-init.sql
    init_sql="$root/tests/compat/postgres-direct-d016/init.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D016/{create.sql,directGraph.ttl}; PostgreSQL-equivalent tests/compat/postgres-direct-d016/init.sql (VARBINARY→bytea, X literal→decode)'
    query=''
    comparison=materialize
    postgres_extensions=none
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations='Patient'
    ;;
  dmd017)
    mapping_file=''
    mapping_behavior='Direct Mapping Unicode composite primary key, blank node, and foreign key'
    artifact_case_id=differential-direct-mapping-d017-unicode-composite-pk-and-fk
    source_case=D017
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D017/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D017/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=pgcrypto
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations_toml='["植物", "成分"]'
    ;;
  dmd021)
    mapping_file=''
    mapping_behavior='Direct Mapping composite foreign key NULL omission to parent primary-key IRI'
    artifact_case_id=differential-direct-mapping-d021-composite-foreign-key-null-omission
    source_case=D021
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D021/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D021/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=none
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations_toml='["Target", "Source"]'
    ;;
  dmd024)
    mapping_file=''
    mapping_behavior='Direct Mapping non-primary composite UNIQUE key with NULL parent row omission'
    artifact_case_id=differential-direct-mapping-d024-non-primary-unique-key-null-parent-row
    source_case=D024
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D024/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D024/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=none
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations_toml='["Target", "Source"]'
    ;;
  dmd025)
    mapping_file=''
    mapping_behavior='Direct Mapping five relations with primary and non-primary foreign keys'
    artifact_case_id=differential-direct-mapping-d025-multi-table-multi-foreign-key
    source_case=D025
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D025/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D025/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=pgcrypto
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations_toml='["Addresses", "Department", "People", "Projects", "TaskAssignments"]'
    ;;
  dmd022)
    mapping_file=''
    mapping_behavior='Direct Mapping composite foreign key to no-primary-key parent blank node'
    artifact_case_id=differential-direct-mapping-d022-composite-foreign-key-to-no-primary-key-parent
    source_case=D022
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D022/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D022/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=pgcrypto
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations_toml='["Target", "Source"]'
    ;;
  dmd023)
    mapping_file=''
    mapping_behavior='Direct Mapping foreign key to non-primary UNIQUE key with primary-key IRI object'
    artifact_case_id=differential-direct-mapping-d023-foreign-key-to-non-primary-unique-key
    source_case=D023
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D023/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D023/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=none
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations_toml='["Target", "Source"]'
    ;;
  dmd006)
    mapping_file=''
    mapping_behavior='Direct Mapping single primary-key IRI subject'
    artifact_case_id=differential-direct-mapping-d006-primary-key-iri
    source_case=D006
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D006/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D006/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=none
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations='Student'
    ;;
  dmd007)
    mapping_file=''
    mapping_behavior='Direct Mapping integer primary-key IRI and text column'
    artifact_case_id=differential-direct-mapping-d007-single-primary-key
    source_case=D007
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D007/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D007/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=none
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations='Student'
    ;;
  dmd008)
    mapping_file=''
    mapping_behavior='Direct Mapping composite primary-key percent-encoded IRI'
    artifact_case_id=differential-direct-mapping-d008-composite-primary-key
    source_case=D008
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D008/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D008/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=none
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations='Student'
    ;;
  dmd015)
    mapping_file=''
    mapping_behavior='Direct Mapping multi-row composite primary-key IRI'
    artifact_case_id=differential-direct-mapping-d015-multi-row-composite-primary-key
    source_case=D015
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D015/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D015/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=none
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations='Country'
    ;;
  dmd009)
    mapping_file=''
    mapping_behavior='Direct Mapping primary-key foreign-key reference and NULL omission'
    artifact_case_id=differential-direct-mapping-d009-primary-key-foreign-key
    source_case=D009
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D009/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D009/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=none
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations_toml='["Sport", "Student"]'
    ;;
  dmd013)
    mapping_file=''
    mapping_behavior='Direct Mapping NULL column omission'
    artifact_case_id=differential-direct-mapping-d013-null-column-omission
    source_case=D013
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D013/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D013/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=none
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations='Person'
    ;;
  dmd010)
    mapping_file=''
    mapping_behavior='Direct Mapping percent-encoded relation and column identifiers'
    artifact_case_id=differential-direct-mapping-d010-identifier-percent-encoding
    source_case=D010
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D010/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D010/{create.sql,directGraph.ttl}; Ontop bootstrap + rtop direct_mapping'
    query=''
    comparison=materialize
    postgres_extensions=none
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations='Country Info'
    ;;
  dmd009http)
    mapping_file=''
    mapping_behavior='Direct Mapping HTTP D009 primary key, foreign key, and NULL omission'
    artifact_case_id=differential-direct-mapping-d009-pk-fk-null-http
    source_case=D009
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D009/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D009/{create.sql,directGraph.ttl}; Ontop bootstrap + endpoint; rtop direct_mapping endpoint'
    query="$root/tests/compat/postgres-http/direct-d009-foreign-key.rq"
    comparison=http-json
    postgres_extensions=none
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations_toml='["Sport", "Student"]'
    ;;
  dmd010http)
    mapping_file=''
    mapping_behavior='Direct Mapping HTTP D010 percent-encoded relation and column identifiers'
    artifact_case_id=differential-direct-mapping-d010-percent-encoding-http
    source_case=D010
    init_file=create.sql
    init_sql="$baseline/test/rdb2rdf-compliance/src/test/resources/D010/create.sql"
    fixture_init_provenance='test/rdb2rdf-compliance/src/test/resources/D010/{create.sql,directGraph.ttl}; Ontop bootstrap + endpoint; rtop direct_mapping endpoint'
    query="$root/tests/compat/postgres-http/direct-d010-percent-encoding.rq"
    comparison=http-json
    postgres_extensions=none
    direct_mapping=true
    direct_mapping_base='http://example.com/base/'
    direct_mapping_relations='Country Info'
    ;;
  *)
    printf '%s\n' "未知 DIFFERENTIAL_CASE：$case_name" >&2
    exit 64
    ;;
esac
direct_mapping=${direct_mapping:-false}
direct_mapping_relations=${direct_mapping_relations:-}
direct_mapping_relations_toml=${direct_mapping_relations_toml:-"[\"$direct_mapping_relations\"]"}

work=$(mktemp -d)
artifacts=${DIFFERENTIAL_ARTIFACT_DIR:-$work/artifacts}
database=rtop-differential-$case_name-$$
network=rtop-differential-net-$$
ontop_endpoint=rtop-ontop-endpoint-$case_name-$$
rtop_endpoint_pid=
cleanup() {
  [ -z "$rtop_endpoint_pid" ] || kill "$rtop_endpoint_pid" >/dev/null 2>&1 || true
  [ -z "$rtop_endpoint_pid" ] || wait "$rtop_endpoint_pid" >/dev/null 2>&1 || true
  docker rm -f "$ontop_endpoint" >/dev/null 2>&1 || true
  docker rm -f "$database" >/dev/null 2>&1 || true
  docker network rm "$network" >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT HUP INT TERM
mkdir -p "$artifacts"

docker network create "$network" >/dev/null
docker run -d --name "$database" --network "$network" --network-alias postgres \
  -e POSTGRES_USER=rtop -e POSTGRES_PASSWORD=rtop -e POSTGRES_DB=rtop_test \
  -p 127.0.0.1::5432 "$postgres_image" >/dev/null
port=$(docker port "$database" 5432/tcp | sed 's/.*://')
until docker exec "$database" psql -U rtop -d rtop_test -c 'SELECT 1' >/dev/null 2>&1; do sleep 1; done
if [ "$postgres_extensions" = postgis ]; then
  postgis_ready=0
  while [ "$postgis_ready" -lt 2 ]; do
    if docker exec "$database" psql -U rtop -d rtop_test -c 'SELECT PostGIS_Full_Version()' >/dev/null 2>&1; then
      postgis_ready=$((postgis_ready + 1))
    else
      postgis_ready=0
    fi
    sleep 1
  done
fi
if [ "${init_stockexchange_dump:-false}" = true ]; then
  # 固定 dump 会创建/切换自己的 database；差分容器已经隔离在 rtop_test 中，
  # 因而仅移除这三类会破坏隔离的 session 指令，保留全部 schema 与数据。
  sed '/^DROP DATABASE /d; /^CREATE DATABASE /d; /^\\connect /d; /^CREATE SCHEMA public;/d' "$init_sql" \
    | docker exec -i "$database" psql -U rtop -d rtop_test >/dev/null
else
  docker exec -i "$database" psql -U rtop -d rtop_test < "$init_sql"
fi
if [ "$postgres_extensions" = pgcrypto ]; then
  docker exec "$database" psql -U rtop -d rtop_test -c 'CREATE EXTENSION IF NOT EXISTS pgcrypto' >/dev/null
fi

cat > "$work/ontop.properties" <<EOF
jdbc.url=jdbc:postgresql://postgres:5432/rtop_test
jdbc.user=rtop
jdbc.password=rtop
jdbc.driver=org.postgresql.Driver
EOF
[ "${ontop_infer_default_datatype:-false}" = true ] && printf '%s\n' 'ontop.inferDefaultDatatype=true' >> "$work/ontop.properties"
if [ "$direct_mapping" = true ]; then
cat > "$work/rtop.toml" <<EOF
[direct_mapping]
base_iri = "$direct_mapping_base"
relations = $direct_mapping_relations_toml
${direct_mapping_preserve_physical_rows:+preserve_physical_rows = "$direct_mapping_preserve_physical_rows"}

[datasource]
kind = "postgres"
host = "127.0.0.1"
port = $port
database = "rtop_test"
user = "rtop"
password = "rtop"
EOF
if [ "$comparison" = http-json ]; then
  docker run --rm --network "$network" \
    -v "$ONTOP_HOME:/opt/ontop:ro" -v "$work:/case" \
    -w /opt/ontop --entrypoint /bin/bash maven:3.9-eclipse-temurin-17 \
    -lc "./ontop bootstrap -b '$direct_mapping_base' -m /case/ontop-direct.obda -t /case/ontop-direct.ttl -p /case/ontop.properties" \
    > "$artifacts/ontop.bootstrap.raw" 2>&1
  ontop_mapping=/case/ontop-direct.obda
fi
else
if [ -n "${mapping_source:-}" ]; then
  mapping_name=${mapping_name:-mapping.obda}
  cp "$mapping_source" "$work/$mapping_name"
  if [ -n "${ontology_source:-}" ]; then
    cp "$ontology_source" "$work/ontology.owl"
  fi
  rtop_mapping="$work/$mapping_name"
  ontop_mapping=/case/$mapping_name
  if [ -n "${ontop_mapping_source:-}" ]; then
    cp "$ontop_mapping_source" "$work/ontop-mapping.obda"
    ontop_mapping=/case/ontop-mapping.obda
  fi
else
  rtop_mapping="$baseline/test/rdb2rdf-compliance/src/test/resources/$source_case/$mapping_file"
  ontop_mapping=/ontop-source/$source_case/$mapping_file
fi
cat > "$work/rtop.toml" <<EOF
mapping = "$rtop_mapping"
${rtop_mapping_infer_default_datatype:+mapping_infer_default_datatype = "$rtop_mapping_infer_default_datatype"}
${rtop_mapping_require_absolute_iri_values:+mapping_require_absolute_iri_values = "$rtop_mapping_require_absolute_iri_values"}

[datasource]
kind = "postgres"
host = "127.0.0.1"
port = $port
database = "rtop_test"
user = "rtop"
password = "rtop"
EOF
if [ -n "${ontology_source:-}" ]; then
  sed -i "1a\\ontology = \"$work/ontology.owl\"" "$work/rtop.toml"
  ontop_endpoint_options="${ontop_endpoint_options:-} -t /case/ontology.owl"
  for ontology_import_source in ${ontology_import_sources:-}; do
    cp "$ontology_import_source" "$work/$(basename "$ontology_import_source")"
  done
  if [ -n "${ontology_catalog_source:-}" ]; then
    cp "$ontology_catalog_source" "$work/imports-catalog.xml"
    cp "$ontology_source" "$work/$(basename "$ontology_source")"
    sed -i "1a\\xml_catalog = \"$work/imports-catalog.xml\"" "$work/rtop.toml"
    ontop_endpoint_options="$ontop_endpoint_options --xml-catalog /case/imports-catalog.xml"
  fi
fi
if [ -n "${ontop_lenses_source:-}" ]; then
  cp "$ontop_lenses_source" "$work/ontop-lenses.json"
  ontop_endpoint_options="${ontop_endpoint_options:-} --lenses /case/ontop-lenses.json"
fi
if [ -n "${facts_source:-}" ]; then
  facts_name=$(basename "$facts_source")
  if [ -e "$facts_source" ]; then
    cp "$facts_source" "$work/$facts_name"
  elif [ "${facts_allow_missing:-false}" != true ]; then
    printf '%s\n' "facts source does not exist: $facts_source" >&2
    exit 1
  fi
  sed -i "1a\\facts = \"$work/$facts_name\"" "$work/rtop.toml"
  ontop_endpoint_options="${ontop_endpoint_options:-} -a /case/$facts_name"
  if [ -n "${facts_format:-}" ]; then
    sed -i "1a\\facts_format = \"$facts_format\"" "$work/rtop.toml"
    ontop_endpoint_options="$ontop_endpoint_options --facts-format $facts_format"
  fi
  if [ -n "${facts_base_iri:-}" ]; then
    sed -i "1a\\facts_base_iri = \"$facts_base_iri\"" "$work/rtop.toml"
    ontop_endpoint_options="$ontop_endpoint_options --facts-base-iri $facts_base_iri"
  fi
fi
if [ "${endpoint_predefined:-false}" = true ]; then
  cp "$root/tests/compat/postgres-http/predefined.json" "$work/predefined.json"
  cp "$root/tests/compat/postgres-http/predefined.toml" "$work/predefined.toml"
  printf '%s\n' '[endpoint]' 'predefined_config = "predefined.json"' 'predefined_queries = "predefined.toml"' >> "$work/rtop.toml"
  ontop_endpoint_options="${ontop_endpoint_options:-} --predefined-config /case/predefined.json --predefined-queries /case/predefined.toml"
fi
if [ "${endpoint_enable_download_ontology:-false}" = true ]; then
  printf '%s\n' '[endpoint]' 'enable_download_ontology = true' >> "$work/rtop.toml"
fi
fi

if [ "$comparison" = query ] || [ "$comparison" = simple-csv ]; then
  cp "$query" "$work/query.rq"
  docker run --rm --network "$network" \
    -v "$ONTOP_HOME:/opt/ontop:ro" -v "$work:/case:ro" \
    -v "$baseline/test/rdb2rdf-compliance/src/test/resources:/ontop-source:ro" \
    -w /opt/ontop \
    --entrypoint /bin/bash maven:3.9-eclipse-temurin-17 \
    -lc "./ontop query -m $ontop_mapping -p /case/ontop.properties -q /case/query.rq -o /tmp/result.csv && cat /tmp/result.csv" \
    > "$artifacts/ontop.raw"
  (cd "$root" && cargo run --quiet -- query "$work/rtop.toml" "$query") > "$artifacts/rtop.raw"
elif [ "$comparison" = materialize ]; then
  if [ "$direct_mapping" = true ]; then
    docker run --rm --network "$network" \
      -v "$ONTOP_HOME:/opt/ontop:ro" -v "$work:/case" \
      -w /opt/ontop \
      --entrypoint /bin/bash maven:3.9-eclipse-temurin-17 \
      -lc "./ontop bootstrap -b '$direct_mapping_base' -m /case/ontop.obda -t /case/ontology.owl -p /case/ontop.properties" \
      > "$artifacts/ontop.bootstrap.raw" 2>&1
    ontop_mapping=/case/ontop.obda
  else
    : "${ontop_mapping:=/ontop-source/$source_case/$mapping_file}"
  fi
  docker run --rm --network "$network" \
    -v "$ONTOP_HOME:/opt/ontop:ro" -v "$work:/case:ro" \
    -v "$baseline/test/rdb2rdf-compliance/src/test/resources:/ontop-source:ro" \
    -w /opt/ontop \
    --entrypoint /bin/bash maven:3.9-eclipse-temurin-17 \
    -lc "./ontop materialize -m $ontop_mapping -p /case/ontop.properties -f nquads -o /tmp/ontop.nq >/tmp/ontop.stdout && cat /tmp/ontop.nq" \
    > "$artifacts/ontop.raw"
  (cd "$root" && cargo run --quiet -- materialize "$work/rtop.toml" "$artifacts/rtop.raw" nquads) > "$artifacts/rtop.stdout"
elif [ "$comparison" = compile ]; then
  docker run --rm --network "$network" \
    -v "$ONTOP_HOME:/opt/ontop:ro" -v "$work:/case:ro" \
    -w /opt/ontop \
    --entrypoint /bin/bash maven:3.9-eclipse-temurin-17 \
    -lc './ontop compile' > "$artifacts/ontop.raw"
  (cd "$root" && cargo run --quiet -- compile "$work/rtop.toml") > "$artifacts/rtop.raw"
elif [ "$comparison" = validate ]; then
  docker run --rm --network "$network" \
    -v "$ONTOP_HOME:/opt/ontop:ro" -v "$work:/case:ro" \
    -w /opt/ontop \
    --entrypoint /bin/bash maven:3.9-eclipse-temurin-17 \
    -lc './ontop validate -m /case/mapping.obda -t /case/ontology.owl -p /case/ontop.properties' > "$artifacts/ontop.raw"
  (cd "$root" && cargo run --quiet -- validate "$work/rtop.toml") > "$artifacts/rtop.raw"
elif [ "$comparison" = bootstrap-materialize ]; then
  docker run --rm --network "$network" \
    -v "$ONTOP_HOME:/opt/ontop:ro" -v "$work:/case" \
    -w /opt/ontop \
    --entrypoint /bin/bash maven:3.9-eclipse-temurin-17 \
    -lc "./ontop bootstrap -b '$bootstrap_base' -m /case/ontop-bootstrap.obda -t /case/ontop-bootstrap.owl -p /case/ontop.properties && ./ontop materialize -m /case/ontop-bootstrap.obda -p /case/ontop.properties -f nquads -o /tmp/ontop.nq >/tmp/ontop.stdout && cat /tmp/ontop.nq" \
    > "$artifacts/ontop.raw"
  (cd "$root" && cargo run --quiet -- bootstrap "$work/rtop.toml" "$bootstrap_base" "$work/rtop-bootstrap.obda" "$work/rtop-bootstrap.ttl") > "$artifacts/rtop.bootstrap.stdout"
  sed "s|^mapping = .*|mapping = \"$work/rtop-bootstrap.obda\"|" "$work/rtop.toml" > "$work/rtop-bootstrap.toml"
  (cd "$root" && cargo run --quiet -- materialize "$work/rtop-bootstrap.toml" "$artifacts/rtop.raw" nquads) > "$artifacts/rtop.stdout"
elif [ "$comparison" = pretty-materialize ]; then
  docker run --rm --network "$network" \
    -v "$ONTOP_HOME:/opt/ontop:ro" -v "$work:/case" \
    -v "$baseline/test/rdb2rdf-compliance/src/test/resources:/ontop-source:ro" \
    -w /opt/ontop \
    --entrypoint /bin/bash maven:3.9-eclipse-temurin-17 \
    -lc "./ontop mapping pretty-r2rml -i /ontop-source/$source_case/$mapping_file -o /case/ontop-pretty.ttl && ./ontop materialize -m /case/ontop-pretty.ttl -p /case/ontop.properties -f nquads -o /tmp/ontop.nq >/tmp/ontop.stdout && cat /tmp/ontop.nq" \
    > "$artifacts/ontop.raw"
  (cd "$root" && cargo run --quiet -- mapping pretty-r2rml "$rtop_mapping" "$work/rtop-pretty.ttl") > "$artifacts/rtop.pretty.stdout"
  sed "s|^mapping = .*|mapping = \"$work/rtop-pretty.ttl\"|" "$work/rtop.toml" > "$work/rtop-pretty.toml"
  (cd "$root" && cargo run --quiet -- materialize "$work/rtop-pretty.toml" "$artifacts/rtop.raw" nquads) > "$artifacts/rtop.stdout"
elif [ "$comparison" = v1-to-v3-materialize ]; then
  docker run --rm --network "$network" \
    -v "$ONTOP_HOME:/opt/ontop:ro" -v "$work:/case" \
    -w /opt/ontop \
    --entrypoint /bin/bash maven:3.9-eclipse-temurin-17 \
    -lc "./ontop mapping v1-to-v3 -m $ontop_mapping -o /case/ontop-converted.$(printf '%s' "$ontop_mapping" | sed 's/.*\.//') $v1_option >/tmp/ontop.convert.stdout && ./ontop materialize -m /case/ontop-converted.$(printf '%s' "$ontop_mapping" | sed 's/.*\.//') -p /case/ontop.properties -f nquads -o /tmp/ontop.nq >/tmp/ontop.stdout && cat /tmp/ontop.nq" \
    > "$artifacts/ontop.raw"
  converted_extension=$(printf '%s' "$rtop_mapping" | sed 's/.*\.//')
  (cd "$root" && cargo run --quiet -- mapping v1-to-v3 "$rtop_mapping" "$work/rtop-converted.$converted_extension" $v1_option) > "$artifacts/rtop.convert.stdout"
  sed "s|^mapping = .*|mapping = \"$work/rtop-converted.$converted_extension\"|" "$work/rtop.toml" > "$work/rtop-converted.toml"
  (cd "$root" && cargo run --quiet -- materialize "$work/rtop-converted.toml" "$artifacts/rtop.raw" nquads) > "$artifacts/rtop.stdout"
elif [ "$comparison" = to-obda-materialize ]; then
  if [ "$source_case" = none ]; then
    ontop_conversion_input=$ontop_mapping
  else
    ontop_conversion_input=/ontop-source/$source_case/$mapping_file
  fi
  docker run --rm --network "$network" \
    -v "$ONTOP_HOME:/opt/ontop:ro" -v "$work:/case" \
    -v "$baseline/test/rdb2rdf-compliance/src/test/resources:/ontop-source:ro" \
    -w /opt/ontop \
    --entrypoint /bin/bash maven:3.9-eclipse-temurin-17 \
    -lc "./ontop mapping to-obda -i $ontop_conversion_input -o /case/ontop-converted.obda && ./ontop materialize -m /case/ontop-converted.obda -p /case/ontop.properties -f nquads -o /tmp/ontop.nq >/tmp/ontop.stdout && cat /tmp/ontop.nq" \
    > "$artifacts/ontop.raw"
  (cd "$root" && cargo run --quiet -- mapping to-obda "$rtop_mapping" "$work/rtop-converted.obda") > "$artifacts/rtop.convert.stdout"
  sed "s|^mapping = .*|mapping = \"$work/rtop-converted.obda\"|" "$work/rtop.toml" > "$work/rtop-converted.toml"
  (cd "$root" && cargo run --quiet -- materialize "$work/rtop-converted.toml" "$artifacts/rtop.raw" nquads) > "$artifacts/rtop.stdout"
elif [ "$comparison" = to-r2rml-materialize ]; then
  docker run --rm --network "$network" \
    -v "$ONTOP_HOME:/opt/ontop:ro" -v "$work:/case" \
    -w /opt/ontop \
    --entrypoint /bin/bash maven:3.9-eclipse-temurin-17 \
    -lc './ontop mapping to-r2rml -i /case/mapping.obda -o /case/ontop-converted.ttl --force >/tmp/ontop.convert.stdout && ./ontop materialize -m /case/ontop-converted.ttl -p /case/ontop.properties -f nquads -o /tmp/ontop.nq >/tmp/ontop.stdout && cat /tmp/ontop.nq' \
    > "$artifacts/ontop.raw"
  (cd "$root" && cargo run --quiet -- mapping to-r2rml "$rtop_mapping" "$work/rtop-converted.ttl" --force) > "$artifacts/rtop.convert.stdout"
  sed "s|^mapping = .*|mapping = \"$work/rtop-converted.ttl\"|" "$work/rtop.toml" > "$work/rtop-converted.toml"
  (cd "$root" && cargo run --quiet -- materialize "$work/rtop-converted.toml" "$artifacts/rtop.raw" nquads) > "$artifacts/rtop.stdout"
elif [ "$comparison" = metadata ]; then
  docker run --rm --network "$network" \
    -v "$ONTOP_HOME:/opt/ontop:ro" -v "$work:/case" \
    -w /opt/ontop \
    --entrypoint /bin/bash maven:3.9-eclipse-temurin-17 \
    -lc './ontop extract-db-metadata -p /case/ontop.properties -o /case/ontop-metadata.json && cat /case/ontop-metadata.json' \
    > "$artifacts/ontop.raw"
  (cd "$root" && cargo run --quiet -- extract-db-metadata "$work/rtop.toml" -) > "$artifacts/rtop.raw"
elif [ "$comparison" = conversion-rejection ]; then
  if docker run --rm --network "$network" \
    -v "$ONTOP_HOME:/opt/ontop:ro" -v "$work:/case:ro" \
    -w /opt/ontop \
    --entrypoint /bin/bash maven:3.9-eclipse-temurin-17 \
    -lc "./ontop mapping to-r2rml -i $ontop_mapping -o /tmp/ontop-converted.ttl --force" \
    > "$artifacts/ontop.raw" 2>&1; then
    ontop_exit=0
  else
    ontop_exit=$?
  fi
  if (cd "$root" && cargo run --quiet -- mapping to-r2rml "$rtop_mapping" "$work/rtop-converted.ttl" --force) \
    > "$artifacts/rtop.raw" 2>&1; then
    rtop_exit=0
  else
    rtop_exit=$?
  fi
elif [ "$comparison" = v1-to-v3-rejection ]; then
  if docker run --rm --network "$network" \
    -v "$ONTOP_HOME:/opt/ontop:ro" -v "$work:/case" \
    -w /opt/ontop \
    --entrypoint /bin/bash maven:3.9-eclipse-temurin-17 \
    -lc "./ontop mapping v1-to-v3 -m $ontop_mapping -o /case/ontop-converted.obda" \
    > "$artifacts/ontop.raw" 2>&1; then
    ontop_exit=0
  else
    ontop_exit=$?
  fi
  # Ontop 的 CLI 在此异常路径仍以 0 退出，且可能已创建空输出文件；stderr
  # 中的稳定错误类别才是该命令实际拒绝输入的唯一外部信号。
  if grep -q 'Error occurred during v1-to-v3 mapping conversion:' "$artifacts/ontop.raw"; then
    ontop_exit=1
  fi
  if (cd "$root" && cargo run --quiet -- mapping v1-to-v3 "$rtop_mapping" "$work/rtop-converted.obda") \
    > "$artifacts/rtop.raw" 2>&1; then
    rtop_exit=0
  else
    rtop_exit=$?
  fi
  [ -e "$work/rtop-converted.obda" ] || rtop_exit=1
elif [ "$comparison" = rejection ] || [ "$comparison" = native-reader-rejection ] || [ "$comparison" = native-source-relation-error ]; then
  if [ "$source_case" = none ]; then
    ontop_rejection_mapping="$ontop_mapping"
  else
    ontop_rejection_mapping="/ontop-source/$source_case/$mapping_file"
  fi
  if docker run --rm --network "$network" \
    -v "$ONTOP_HOME:/opt/ontop:ro" -v "$work:/case:ro" \
    -v "$baseline/test/rdb2rdf-compliance/src/test/resources:/ontop-source:ro" \
    -w /opt/ontop \
    --entrypoint /bin/bash maven:3.9-eclipse-temurin-17 \
    -lc "./ontop materialize -m $ontop_rejection_mapping -p /case/ontop.properties -f nquads -o /tmp/ontop.nq" \
    > "$artifacts/ontop.raw" 2>&1; then
    ontop_exit=0
  else
    ontop_exit=$?
  fi
  # CLI 错误输出本身是 rtop 的外部 rejection 证据；materialization 目标使用
  # 临时路径，避免同一文件被程序输出和 shell 重定向同时占用。
  if (cd "$root" && cargo run --quiet -- materialize "$work/rtop.toml" "$work/rtop.nq" nquads) \
    > "$artifacts/rtop.raw" 2>&1; then
    rtop_exit=0
  else
    rtop_exit=$?
  fi
else
  docker run -d --name "$ontop_endpoint" --network "$network" -p 127.0.0.1::8080 \
    -v "$ONTOP_HOME:/opt/ontop:ro" -v "$work:/case:ro" \
    -v "$baseline/test/rdb2rdf-compliance/src/test/resources:/ontop-source:ro" \
    -w /opt/ontop --entrypoint /bin/bash maven:3.9-eclipse-temurin-17 \
    -lc "./ontop endpoint -m $ontop_mapping -p /case/ontop.properties --port 8080 --disable-portal-page ${ontop_endpoint_options:-}" >/dev/null
  if ! docker inspect --format '{{.State.Running}}' "$ontop_endpoint" | grep -qx true; then
    docker logs "$ontop_endpoint" > "$artifacts/ontop.endpoint.log" 2>&1
    exit 1
  fi
  if ! ontop_port=$(docker port "$ontop_endpoint" 8080/tcp 2>/dev/null | sed 's/.*://'); then
    docker logs "$ontop_endpoint" > "$artifacts/ontop.endpoint.log" 2>&1
    exit 1
  fi
  if [ -z "$ontop_port" ]; then
    docker logs "$ontop_endpoint" > "$artifacts/ontop.endpoint.log" 2>&1
    exit 1
  fi
  rtop_port=$((20000 + ($$ % 10000)))
  if [ "${rtop_development:-false}" = true ]; then
    (cd "$root" && RTOP_DEVELOPMENT=1 cargo run --quiet -- endpoint "$work/rtop.toml" "127.0.0.1:$rtop_port") > "$artifacts/rtop.endpoint.log" 2>&1 &
  else
    (cd "$root" && cargo run --quiet -- endpoint "$work/rtop.toml" "127.0.0.1:$rtop_port") > "$artifacts/rtop.endpoint.log" 2>&1 &
  fi
  rtop_endpoint_pid=$!
  attempts=0
  if [ "${endpoint_health:-sparql}" = actuator ]; then
    ontop_health_url="http://127.0.0.1:$ontop_port/actuator/health"
  else
    ontop_health_url="http://127.0.0.1:$ontop_port/sparql?query=ASK%20%7B%20%3Fs%20%3Fp%20%3Fo%20%7D"
  fi
  until curl -fsS "$ontop_health_url" >/dev/null 2>&1; do
    # 端口在 Spring 初始化初期即可被发布；若随后映射装载失败，不能把已退出的
    # Ontop 容器误报成普通超时。立即保留其日志，形成可重放的失败指纹。
    if ! docker inspect --format '{{.State.Running}}' "$ontop_endpoint" | grep -qx true; then
      docker logs "$ontop_endpoint" > "$artifacts/ontop.endpoint.log" 2>&1
      exit 1
    fi
    attempts=$((attempts + 1)); [ "$attempts" -lt "${endpoint_health_attempts:-60}" ] || { docker logs "$ontop_endpoint" > "$artifacts/ontop.endpoint.log" 2>&1; exit 1; }; sleep 1
  done
  attempts=0
  until curl -fsS "http://127.0.0.1:$rtop_port/healthz" >/dev/null 2>&1; do
    attempts=$((attempts + 1)); [ "$attempts" -lt 60 ] || { cat "$artifacts/rtop.endpoint.log" >&2; exit 1; }; sleep 1
  done
  if [ "${DIFFERENTIAL_CAPTURE_REFORMULATION:-false}" = true ] && [ -f "${query:-}" ]; then
    curl -sS --get --data-urlencode "query=$(cat "$query")" \
      "http://127.0.0.1:$ontop_port/ontop/reformulate" > "$artifacts/ontop.reformulation.sql"
  fi
  if [ "$comparison" = http-status ]; then
    curl -sS -o "$artifacts/ontop.body" -w '%{http_code}\n' "http://127.0.0.1:$ontop_port/ontology" > "$artifacts/ontop.raw"
    curl -sS -o "$artifacts/rtop.body" -w '%{http_code}\n' "http://127.0.0.1:$rtop_port/ontology" > "$artifacts/rtop.raw"
  elif [ "$comparison" = http-status-body ]; then
    curl -sS -o "$artifacts/ontop.body" -w '%{http_code}\n' "http://127.0.0.1:$ontop_port/ontology" > "$artifacts/ontop.raw"
    curl -sS -o "$artifacts/rtop.body" -w '%{http_code}\n' "http://127.0.0.1:$rtop_port/ontology" > "$artifacts/rtop.raw"
    ontop_post_status=$(curl -sS -X POST -o "$artifacts/ontop.post.body" -w '%{http_code}' "http://127.0.0.1:$ontop_port/ontology")
    rtop_post_status=$(curl -sS -X POST -o "$artifacts/rtop.post.body" -w '%{http_code}' "http://127.0.0.1:$rtop_port/ontology")
    printf '%s\n' "$ontop_post_status" > "$artifacts/ontop.post.status"
    printf '%s\n' "$rtop_post_status" > "$artifacts/rtop.post.status"
  elif [ "$comparison" = http-ontology-content ]; then
    curl -sS -o "$artifacts/ontop.body" -w '%{http_code}\n' "http://127.0.0.1:$ontop_port/ontology" > "$artifacts/ontop.raw"
    curl -sS -o "$artifacts/rtop.body" -w '%{http_code}\n' "http://127.0.0.1:$rtop_port/ontology" > "$artifacts/rtop.raw"
    ontop_post_status=$(curl -sS -X POST -o "$artifacts/ontop.post.body" -w '%{http_code}' "http://127.0.0.1:$ontop_port/ontology")
    rtop_post_status=$(curl -sS -X POST -o "$artifacts/rtop.post.body" -w '%{http_code}' "http://127.0.0.1:$rtop_port/ontology")
    printf '%s\n' "$ontop_post_status" > "$artifacts/ontop.post.status"
    printf '%s\n' "$rtop_post_status" > "$artifacts/rtop.post.status"
  elif [ "$comparison" = http-predefined ]; then
    curl -fsS -H 'Accept: text/turtle' \
      "http://127.0.0.1:$ontop_port/predefined/person?person=https%3A%2F%2Fexample.test%2Fperson%2F1" \
      > "$artifacts/ontop.raw"
    curl -fsS -H 'Accept: text/turtle' \
      "http://127.0.0.1:$rtop_port/predefined/person?person=https%3A%2F%2Fexample.test%2Fperson%2F1" \
      > "$artifacts/rtop.raw"
  elif [ "$comparison" = http-predefined-invalid ]; then
    curl -sS -o "$artifacts/ontop.body" -w '%{http_code}\n' \
      "http://127.0.0.1:$ontop_port/predefined/person?person=not-an-iri" > "$artifacts/ontop.raw"
    curl -sS -o "$artifacts/rtop.body" -w '%{http_code}\n' \
      "http://127.0.0.1:$rtop_port/predefined/person?person=not-an-iri" > "$artifacts/rtop.raw"
  elif [ "$comparison" = http-protocol ]; then
    protocol_query='ASK { ?s a <http://it.unibz.inf/obda/test/simple#A> }'
    protocol_capture() {
      endpoint=$1; output=$2
      get=$(curl -sS -o "$output.get.body" -w '%{http_code}' -G --data-urlencode "query=$protocol_query" -H 'Accept: application/sparql-results+json' "$endpoint/sparql")
      form=$(curl -sS -o "$output.form.body" -w '%{http_code}' -X POST --data-urlencode "query=$protocol_query" -H 'Accept: application/sparql-results+json' "$endpoint/sparql")
      sparql=$(curl -sS -o "$output.sparql.body" -w '%{http_code}' -X POST --data-binary "$protocol_query" -H 'Content-Type: application/sparql-query' -H 'Accept: application/sparql-results+json' "$endpoint/sparql")
      jq -n --arg get "$get" --arg form "$form" --arg sparql "$sparql" --argjson get_body "$(cat "$output.get.body")" --argjson form_body "$(cat "$output.form.body")" --argjson sparql_body "$(cat "$output.sparql.body")" '{get:{status:$get,body:$get_body},form_post:{status:$form,body:$form_body},sparql_post:{status:$sparql,body:$sparql_body}}' > "$output"
    }
    protocol_capture "http://127.0.0.1:$ontop_port" "$artifacts/ontop.raw"
    protocol_capture "http://127.0.0.1:$rtop_port" "$artifacts/rtop.raw"
  elif [ "$comparison" = http-result-formats ]; then
    formats_select_query='SELECT ?person ?name WHERE { ?person <https://example.test/type> <https://example.test/Person> . ?person <https://example.test/name> ?name . } LIMIT 1'
    formats_construct_query='CONSTRUCT { ?person <https://example.test/name> ?name } WHERE { ?person <https://example.test/name> ?name }'
    formats_capture() {
      endpoint=$1; output=$2
      for format in json xml csv tsv; do
        case "$format" in
          json) accept='application/sparql-results+json' ;;
          xml) accept='application/sparql-results+xml' ;;
          csv) accept='text/csv' ;;
          tsv) accept='text/tab-separated-values' ;;
        esac
        curl -sS -o "$output.$format.body" -w '%{http_code}' --get --data-urlencode "query=$formats_select_query" -H "Accept: $accept" "$endpoint/sparql" > "$output.$format.status"
      done
      curl -sS -o "$output.ntriples.body" -w '%{http_code}' --get --data-urlencode "query=$formats_construct_query" -H 'Accept: application/n-triples' "$endpoint/sparql" > "$output.ntriples.status"
      curl -sS -o "$output.unsupported.body" -w '%{http_code}' --get --data-urlencode "query=$formats_select_query" -H 'Accept: application/x-rtop-unsupported' "$endpoint/sparql" > "$output.unsupported.status"
      jq -n \
        --arg json_status "$(cat "$output.json.status")" --rawfile json "$output.json.body" \
        --arg xml_status "$(cat "$output.xml.status")" --rawfile xml "$output.xml.body" \
        --arg csv_status "$(cat "$output.csv.status")" --rawfile csv "$output.csv.body" \
        --arg tsv_status "$(cat "$output.tsv.status")" --rawfile tsv "$output.tsv.body" \
        --arg ntriples_status "$(cat "$output.ntriples.status")" --rawfile ntriples "$output.ntriples.body" \
        --arg unsupported_status "$(cat "$output.unsupported.status")" --rawfile unsupported "$output.unsupported.body" \
        '{json:{status:$json_status,body:$json},xml:{status:$xml_status,body:$xml},csv:{status:$csv_status,body:$csv},tsv:{status:$tsv_status,body:$tsv},ntriples:{status:$ntriples_status,body:$ntriples},unsupported:{status:$unsupported_status,body:$unsupported}}' > "$output"
    }
    formats_capture "http://127.0.0.1:$ontop_port" "$artifacts/ontop.raw"
    formats_capture "http://127.0.0.1:$rtop_port" "$artifacts/rtop.raw"
  elif [ "$comparison" = http-concurrency-isolation ]; then
    delayed_query=$(cat "$root/tests/compat/postgres-concurrency/delayed.rq")
    ask_query='ASK { ?person <https://example.test/type> <https://example.test/Person> }'
    concurrency_capture() {
      endpoint=$1; output=$2
      curl -sS -o "$output.delayed.body" -w '%{http_code}' -G --data-urlencode "query=$delayed_query" "$endpoint/sparql" > "$output.delayed.status" &
      delayed_pid=$!
      sleep 1
      ask_status=$(curl -sS -o "$output.ask.body" -w '%{http_code}' -G --data-urlencode "query=$ask_query" "$endpoint/sparql")
      wait "$delayed_pid"
      set +e
      curl -sS --max-time 1 -o "$output.disconnect.body" -G --data-urlencode "query=$delayed_query" "$endpoint/sparql"
      disconnect_exit=$?
      set -e
      sleep 1
      active_delays=$(docker exec "$database" psql -U rtop -d rtop_test -Atc "SELECT count(*) FROM pg_stat_activity WHERE pid <> pg_backend_pid() AND query LIKE 'SELECT 1 AS id FROM (SELECT pg_sleep%'")
      reuse_status=$(curl -sS -o "$output.reuse.body" -w '%{http_code}' -G --data-urlencode "query=$ask_query" "$endpoint/sparql")
      jq -n --arg delayed_status "$(cat "$output.delayed.status")" --rawfile delayed "$output.delayed.body" --arg ask_status "$ask_status" --rawfile ask "$output.ask.body" --argjson disconnect_exit "$disconnect_exit" --arg active_delays "$active_delays" --arg reuse_status "$reuse_status" --rawfile reuse "$output.reuse.body" '{delayed:{status:$delayed_status,body:$delayed},ask:{status:$ask_status,body:$ask},disconnect:{curl_exit:$disconnect_exit,active_delays:$active_delays},reuse:{status:$reuse_status,body:$reuse}}' > "$output"
    }
    concurrency_capture "http://127.0.0.1:$ontop_port" "$artifacts/ontop.raw"
    concurrency_capture "http://127.0.0.1:$rtop_port" "$artifacts/rtop.raw"
  elif [ "$comparison" = http-reformulate ]; then
    reformulate_capture() {
      endpoint=$1; name=$2
      default_status=$(curl -sS -D "$artifacts/$name.default.headers" -o "$artifacts/$name.default.body" -w '%{http_code}' --get --data-urlencode "query=$(cat "$query")" "$endpoint/ontop/reformulate")
      native_status=$(curl -sS -D "$artifacts/$name.native.headers" -o "$artifacts/$name.native.body" -w '%{http_code}' --get --data-urlencode "query=$(cat "$query")" --data-urlencode 'forNativeConsumption=true' "$endpoint/ontop/reformulate")
      jq -n \
        --arg default_status "$default_status" --arg native_status "$native_status" \
        --arg default_body "$(cat "$artifacts/$name.default.body")" --arg native_body "$(cat "$artifacts/$name.native.body")" \
        --arg default_query_id "$(awk 'BEGIN{IGNORECASE=1} /^X-Query-ID:/ {print $2}' "$artifacts/$name.default.headers" | tr -d '\r')" \
        --arg native_query_id "$(awk 'BEGIN{IGNORECASE=1} /^X-Query-ID:/ {print $2}' "$artifacts/$name.native.headers" | tr -d '\r')" \
        '{default:{status:$default_status,body:$default_body,query_id:$default_query_id},native:{status:$native_status,body:$native_body,query_id:$native_query_id}}' > "$artifacts/$name.raw"
    }
    reformulate_capture "http://127.0.0.1:$ontop_port" ontop
    reformulate_capture "http://127.0.0.1:$rtop_port" rtop
  elif [ "$comparison" = http-error ] || [ "$comparison" = http-native-source-sql-error ] || [ "$comparison" = http-parse-error ] || [ "$comparison" = http-invalid-datetime-lexical ] || [ "$comparison" = http-invalid-boolean-lexical ]; then
    curl -sS -o "$artifacts/ontop.body" -w '%{http_code}\n' --get --data-urlencode "query=$(cat "$query")" "http://127.0.0.1:$ontop_port/sparql" > "$artifacts/ontop.raw"
    curl -sS -o "$artifacts/rtop.body" -w '%{http_code}\n' --get --data-urlencode "query=$(cat "$query")" "http://127.0.0.1:$rtop_port/sparql" > "$artifacts/rtop.raw"
  elif [ "$comparison" = http-service-error ]; then
    curl -sS -o "$artifacts/ontop.body" -w '%{http_code}\n' --get --data-urlencode "query=$query" "http://127.0.0.1:$ontop_port/sparql" > "$artifacts/ontop.raw"
    curl -sS -o "$artifacts/rtop.body" -w '%{http_code}\n' --get --data-urlencode "query=$query" "http://127.0.0.1:$rtop_port/sparql" > "$artifacts/rtop.raw"
  else
    cp "$query" "$work/query.rq"
    curl -sS --get --data-urlencode "query=$(cat "$query")" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$ontop_port/sparql" > "$artifacts/ontop.raw"
    curl -sS --get --data-urlencode "query=$(cat "$query")" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$rtop_port/sparql" > "$artifacts/rtop.raw"
  fi
fi

if [ "$comparison" = rejection ] || [ "$comparison" = native-reader-rejection ] || [ "$comparison" = native-source-relation-error ] || [ "$comparison" = conversion-rejection ] || [ "$comparison" = v1-to-v3-rejection ]; then
  if [ "$comparison" = native-reader-rejection ]; then
    grep -qi 'Invalid target' "$artifacts/ontop.raw"
    grep -qi 'invalid-mapping:' "$artifacts/rtop.raw"
    normalized_format=native-mapping-reader-rejection
  elif [ "$comparison" = native-source-relation-error ]; then
    # 固定 Ontop CLI 在 materialize 的 mapping-to-metadata 阶段将 PostgreSQL
    # 缺失 relation 报为 InvalidMappingSourceQueriesException（Cannot find
    # relation）；rtop 让 PostgreSQL adapter 在执行期给出稳定 datasource 类别。
    # 不要求 rtop 复制 JDBC/Java 的 relation 文本。
    grep -qi 'Cannot find relation.*person' "$artifacts/ontop.raw"
    grep -qi 'datasource-failure: db error' "$artifacts/rtop.raw"
    normalized_format=native-source-relation-runtime-error
  elif [ "$comparison" = conversion-rejection ]; then
    normalized_format=conversion-rejection
  elif [ "$comparison" = v1-to-v3-rejection ]; then
    normalized_format=v1-to-v3-rejection
  else
    normalized_format=mapping-rejection
  fi
  jq -n --argjson ontop_exit "$ontop_exit" --argjson rtop_exit "$rtop_exit" \
    --arg format "$normalized_format" \
    '{format:$format, ontop_exit:$ontop_exit, rtop_exit:$rtop_exit, both_rejected:($ontop_exit != 0 and $rtop_exit != 0)}' \
    > "$artifacts/normalized.json"
  if [ "$ontop_exit" -ne 0 ] && [ "$rtop_exit" -ne 0 ]; then
    comparison_outcome=passed
  else
    comparison_outcome=failed
  fi
elif [ "$comparison" = http-result-formats ]; then
  # serializer 的 XML declaration、CSV 行终止符等 wire lexical 不属于 RDF
  # 差异；固定 person/name fixture 验收每种格式的状态、term 与表格头。
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
  if [ "$formats_match" = true ]; then comparison_outcome=passed; else comparison_outcome=failed; fi
elif [ "$comparison" = http-concurrency-isolation ]; then
  isolation_match=true
  for side in ontop rtop; do
    if ! jq -e '.delayed.status == "200" and (.delayed.body | fromjson | .boolean == false) and .ask.status == "200" and (.ask.body | fromjson | .boolean == true) and .disconnect.curl_exit == 28 and .disconnect.active_delays == "0" and .reuse.status == "200" and (.reuse.body | fromjson | .boolean == true)' "$artifacts/$side.raw" >/dev/null; then isolation_match=false; fi
  done
  jq -n '{format:"http-concurrency-isolation-and-disconnect",delayed:"pg_sleep(5) ASK false",concurrent_ask:true,disconnect_curl_exit:28,active_delays:0,reuse_ask:true}' > "$artifacts/normalized.json"
  if [ "$isolation_match" = true ]; then comparison_outcome=passed; else comparison_outcome=failed; fi
elif [ "$case_name" = httpnativetermsnull ]; then
  # HTTP Results JSON 的对象成员顺序不是协议语义。此 fixture 特意同时覆盖
  # xsd:string、language literal 与缺失（NULL）binding，故以 term 字段精确比较，
  # 而不改变由既有差分证据哈希锚定的通用 normalizer。
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
  if [ "$terms_null_match" = true ]; then comparison_outcome=passed; else comparison_outcome=failed; fi
elif "$root/scripts/normalize-ontop-rtop-differential.sh" "$comparison" "$case_name" "$artifacts" "${student_kind:-}"; then
  comparison_outcome=passed
else
  comparison_outcome=failed
fi

postgres_digest=$(docker image inspect "$postgres_image" --format '{{range .RepoDigests}}{{println .}}{{end}}' | awk -F@ '/@sha256:/ { print $2; exit }')
query_provenance=${query:-materialize dataset as N-Quads}
query_provenance=${query_provenance#"$root"/}
required_postgres_digest=${required_postgres_digest:-sha256:5c855ad7b85e68e48a62f34662853f38b57c1c1d80f3a927ab58034fd6d31c5e}
fixture_provenance=${fixture_init_provenance:-"test/rdb2rdf-compliance/src/test/resources/$source_case/{$init_file,$mapping_file}"}
if [ "$comparison_outcome" = failed ]; then
  outcome=failed
  outcome_reason='固定环境下 Ontop 与 rtop 的规范化外部结果不一致'
elif [ "$postgres_digest" = "$required_postgres_digest" ]; then
  outcome=passed
  outcome_reason='固定 PostgreSQL 17 digest 下的双边结果一致'
else
  outcome=environment-uncertain
  outcome_reason='实际 PostgreSQL 内容 digest 与认证 digest 不同；不得作为 passed 证据'
fi
jq -n \
  --arg ontop_commit "$commit" \
  --arg postgres_digest "$postgres_digest" \
  --arg required_postgres_digest "$required_postgres_digest" \
  --arg outcome "$outcome" --arg outcome_reason "$outcome_reason" \
  --arg fixture "$fixture_provenance" \
  --arg case_id "$artifact_case_id" --arg mapping_behavior "$mapping_behavior" \
  --arg query "$query_provenance" \
  --arg postgres_extensions "$postgres_extensions" \
  --arg ontop_raw_sha256 "$(sha256sum "$artifacts/ontop.raw" | awk '{print $1}')" \
  --arg rtop_raw_sha256 "$(sha256sum "$artifacts/rtop.raw" | awk '{print $1}')" \
  --arg normalized_sha256 "$(sha256sum "$artifacts/normalized.json" | awk '{print $1}')" \
  '{schema_version:1, case_id:$case_id, mapping_behavior:$mapping_behavior, status:$outcome, status_reason:$outcome_reason,
    ontop_commit:$ontop_commit, postgres_image_digest:$postgres_digest, required_postgres_image_digest:$required_postgres_digest, fixture:$fixture, query:$query, postgres_extensions:$postgres_extensions,
    independent_processes:{ontop:"Java 17 container CLI",rtop:"Rust host CLI"},
    sha256:{ontop_raw:$ontop_raw_sha256,rtop_raw:$rtop_raw_sha256,normalized:$normalized_sha256}}' \
  > "$artifacts/provenance.json"

if [ "$outcome" != passed ]; then
  printf '%s\n' "differential $case_name/$variant: $outcome; artifacts=$artifacts" >&2
  exit 1
fi
printf '%s\n' "differential $case_name/$variant: passed; artifacts=$artifacts"
