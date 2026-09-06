-- CastPostgreSQLTest 的固定 books 输入。publication_date 与该 Java 测试的
-- 四个可观察 xsd:date 结果一致，供 Ontop 与 rtop HTTP 端点独立差分。
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
  (1, 'SPARQL Tutorial', 43, 0.2, 'good', 'en', TIMESTAMP '2014-06-05 16:47:52'),
  (2, 'The Semantic Web', 23, 0.25, 'bad', 'en', TIMESTAMP '2011-12-08 11:30:00'),
  (3, 'Crime and Punishment', 34, 0.2, 'good', 'en', TIMESTAMP '2015-09-21 09:23:06'),
  (4, 'The Logic Book: Introduction, Second Edition', 10, 0.15, 'good', 'en', TIMESTAMP '1970-11-05 07:50:00');
