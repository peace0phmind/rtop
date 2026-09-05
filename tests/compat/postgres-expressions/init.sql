-- RegexPostgresSQLTest 的最小 stockexchange fixture。source SQL 本身使用
-- PostgreSQL `~*` 和 `!~*`，因此由服务器而非 Rust 字符串匹配实现判定。
CREATE TABLE address (
  id integer PRIMARY KEY,
  street text NOT NULL,
  number integer NOT NULL,
  city text NOT NULL,
  state text NOT NULL,
  country text NOT NULL
);
CREATE TABLE person (
  id integer PRIMARY KEY,
  name text NOT NULL,
  lastname text NOT NULL,
  dateofbirth date NOT NULL,
  ssn text NOT NULL
);
INSERT INTO address VALUES
  (1, 'Via Marconi', 3, 'Bolzano', 'Bolzano', 'Italy'),
  (2, 'Huberg Strasse', 3, 'BOLZANO', 'Bolzano', 'Italy'),
  (3, 'Road street', 24, 'Chonala', 'Veracruz', 'Mexico');
INSERT INTO person VALUES
  (1, 'Joana', 'Filtered', DATE '1970-07-14', 'one'),
  (2, 'Walter', 'Schmidt', DATE '1968-09-03', 'two'),
  (3, 'Patricia', 'Lombrardi', DATE '1975-02-22', 'three'),
  (4, 'Ted', 'Example', DATE '1980-01-01', 'four');

-- 固定 Ontop pgsql/bind/sparqlBindPostgreSQL.obda 的 books 输入，供
-- ofn:*Between 在 PostgreSQL mapping dateTime literal 上取证。
CREATE TABLE books (
  id integer PRIMARY KEY,
  title varchar(100),
  price integer,
  discount numeric(38,2),
  description varchar(100),
  lang varchar(100),
  publication_date timestamp
);
INSERT INTO books VALUES
  (1, 'SPARQL Tutorial', 43, 0.2, 'good', 'en', TIMESTAMP '2014-07-14 11:47:52'),
  (2, 'The Semantic Web', 23, 0.25, 'bad', 'en', TIMESTAMP '2011-12-08 12:30:00'),
  (3, 'Crime and Punishment', 34, 0.2, 'good', 'en', TIMESTAMP '2015-09-21 10:23:06'),
  (4, 'The Logic Book: Introduction, Second Edition', 10, 0.15, 'good', 'en', TIMESTAMP '1967-11-05 07:50:00');
