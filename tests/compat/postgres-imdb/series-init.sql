-- 固定 UnboundVariableIMDbTest 的 ontologyIMDBSimplify.obda 只读取 kind_id=2。
-- 在基础 IMDB gate 之后追加，避免改变其精确 Movie/TV title 断言。
INSERT INTO title (id, title, production_year, kind_id, episode_of_id, season_nr)
SELECT 8000000 + value, 'Series ' || value, 2000 + value, 2, NULL, NULL
FROM generate_series(1, 10) AS value;
