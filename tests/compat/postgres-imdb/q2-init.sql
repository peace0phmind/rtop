-- ImdbPostgresTest.testQ2：Action 是 Brute_Action（进而是 Actionreach）。
-- basic-init.sql 已提供 24 个匹配标题；与这里的 55,027 条合计正好固定
-- 基线断言的 57,216 行，避免共享 fixture 让计数漂移。
INSERT INTO title (id, title, production_year, kind_id, episode_of_id, season_nr)
SELECT 9200000 + value, 'Actionreach film ' || value, 2000, 1, NULL, NULL
FROM generate_series(1, 55027) AS value;
INSERT INTO movie_info (movie_id, info, info_type_id)
SELECT 9200000 + value, 'Action', 3 FROM generate_series(1, 55027) AS value;
