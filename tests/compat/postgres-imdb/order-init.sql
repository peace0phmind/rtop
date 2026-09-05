-- ImdbPostgresTest 的两条 rating ORDER BY 查询；在基础 gate 后追加，避免改变原始小计数。
INSERT INTO title (id, title, production_year, kind_id, episode_of_id, season_nr)
SELECT 9000000 + value, 'Rated action ' || value, 2000 + value, 1, NULL, NULL
FROM generate_series(1, 24) AS value;
INSERT INTO movie_info (movie_id, info, info_type_id)
SELECT 9000000 + value, 'Action', 3 FROM generate_series(1, 24) AS value;
INSERT INTO movie_info_idx (movie_id, info, info_type_id)
SELECT 9000000 + value, (9.0 - value / 100.0)::text, 101
FROM generate_series(1, 24) AS value;
