-- ImdbPostgresTest.testQ1：Tarantino 作为导演关联 18 部电影。
INSERT INTO title (id, title, production_year, kind_id, episode_of_id, season_nr)
SELECT 9100000 + value, 'Tarantino film ' || value, 2000, 1, NULL, NULL
FROM generate_series(1, 17) AS value;
INSERT INTO cast_info (person_id, movie_id, role_id)
SELECT 6, 9100000 + value, 8 FROM generate_series(1, 17) AS value;
