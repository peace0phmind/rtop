-- ImdbPostgresTest.testQ3：Finding Nemo 的 42 名 actor 都有 birthName/birthDate。
-- 该场景复用基础 IMDB 表，但其既有 cast 行会让 inverse property 查询按 bag
-- 语义重复绑定；先恢复此标题的独立输入，再植入基线的 42 人数据集。
DELETE FROM cast_info WHERE movie_id = 1;
INSERT INTO name (id, name)
SELECT value, 'Q3 performer ' || value FROM generate_series(28, 42) AS value;
INSERT INTO cast_info (person_id, movie_id, role_id)
SELECT value, 1, 1 FROM generate_series(1, 42) AS value;
INSERT INTO person_info (person_id, info, info_type_id)
SELECT value, '1970-01-01', 21 FROM generate_series(1, 42) AS value;
