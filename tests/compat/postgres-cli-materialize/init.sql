-- 固定 Ontop CLI simplemapping.obda 的 PostgreSQL 输入：两条 SQL source
-- 分别命中 id < 5 和 id = 7，覆盖 materialize 的多个 mapping assertion。
CREATE TABLE table1 (id INTEGER PRIMARY KEY);
INSERT INTO table1 (id) VALUES (1), (7);
