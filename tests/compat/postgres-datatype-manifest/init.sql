CREATE TABLE datatype_manifest_values (
  id integer PRIMARY KEY,
  boolean_value boolean NOT NULL,
  char_value char(6) NOT NULL,
  numeric_value numeric(10, 2) NOT NULL,
  real_value real NOT NULL,
  double_value double precision NOT NULL,
  date_value date NOT NULL,
  time_value time NOT NULL,
  timetz_value time with time zone NOT NULL,
  timestamp_value timestamp NOT NULL,
  timestamptz_value timestamp with time zone NOT NULL,
  interval_value interval NOT NULL,
  manifest_char char(1) NOT NULL,
  varchar_value varchar(16) NOT NULL,
  text_value text NOT NULL,
  character_value character(1) NOT NULL,
  name_value name NOT NULL,
  smallint_value smallint NOT NULL,
  integer_value integer NOT NULL,
  bigint_value bigint NOT NULL,
  serial_value serial NOT NULL,
  bigserial_value bigserial NOT NULL
);
INSERT INTO datatype_manifest_values VALUES
  (1, true, 'Ada', 1.00, 2.5, 3.5, DATE '2013-03-18', TIME '10:12:10', TIMETZ '18:12:10-08:00', TIMESTAMP '2013-03-18 10:12:10', TIMESTAMPTZ '2013-03-19 03:12:10+01', INTERVAL '100 seconds', 'a', 'abc', 'abc', 'a', 'abc', 1, 1, 1, 1, 1);

-- 以下四表的列名与固定 Ontop PostgreSQL datatype mapping 完全一致，供原始 .obda/.rq
-- 以只读方式直接执行，而非以 rtop 自定义 mapping 代替。
CREATE TABLE "Booleans" (id integer PRIMARY KEY, type_boolean boolean NOT NULL);
INSERT INTO "Booleans" VALUES (1, true), (2, false);
CREATE TABLE "Binaries" (id integer PRIMARY KEY, type_bit bit(1), type_bitvarying bit varying(8));
INSERT INTO "Binaries" VALUES (1, B'1', B'1');

CREATE TABLE "Characters" (
  id integer PRIMARY KEY,
  type_varchar varchar(100) NOT NULL,
  type_character character(1) NOT NULL,
  type_text text NOT NULL,
  type_char char(1) NOT NULL,
  type_name name NOT NULL
);
INSERT INTO "Characters" VALUES (1, 'abc', 'a', 'abc', 'a', 'abc');

CREATE TABLE "Numerics" (
  id integer PRIMARY KEY,
  type_smallint smallint NOT NULL,
  type_integer integer NOT NULL,
  type_bigint bigint NOT NULL,
  type_numeric numeric(16, 5) NOT NULL,
  type_real real NOT NULL,
  type_double double precision NOT NULL,
  type_serial serial NOT NULL,
  type_bigserial bigserial NOT NULL
);
INSERT INTO "Numerics" (id, type_smallint, type_integer, type_bigint, type_numeric, type_real, type_double, type_serial, type_bigserial)
VALUES (1, 1, 1, 1, 1.00000, 1.0, 1.0, 1, 1);

CREATE TABLE "DateTimes" (
  id integer PRIMARY KEY,
  type_timestamp timestamp NOT NULL,
  type_timestamp_tz timestamp with time zone NOT NULL,
  type_date date NOT NULL,
  type_time time NOT NULL,
  type_time_tz time with time zone NOT NULL,
  type_interval interval NOT NULL
);
INSERT INTO "DateTimes" VALUES (1, TIMESTAMP '2013-03-18 10:12:10', TIMESTAMPTZ '2013-03-19 03:12:10+01', DATE '2013-03-18', TIME '10:12:10', TIMETZ '18:12:10-08:00', INTERVAL '100 seconds');

-- 未引用的 PostgreSQL identifier 在服务器端折叠为小写；mapping 刻意保留大写输入。
CREATE TABLE tb_books (bk_title text);
INSERT INTO tb_books VALUES ('a');

CREATE TABLE name (id integer PRIMARY KEY, name text NOT NULL);
INSERT INTO name VALUES (222, 'A.J.');
CREATE TABLE title (id integer PRIMARY KEY, title text NOT NULL, production_year integer NOT NULL);
INSERT INTO title VALUES (97263, 'Colleen', 2001);
CREATE TABLE company_name (id integer PRIMARY KEY, country_code text NOT NULL);
INSERT INTO company_name VALUES (7, '[jp]');

-- QuotedAliasTableTest: retain the quoted table aliases and projected aliases
-- used by the fixed NPD baseline mapping.  One row per source table deliberately
-- exercises its Cartesian-product source without claiming the full NPD corpus.
CREATE TABLE company_reserves ("fldNpdidField" text NOT NULL);
INSERT INTO company_reserves VALUES ('42');
CREATE TABLE company ("cmpNpdidCompany" text NOT NULL);
INSERT INTO company VALUES ('acme');
