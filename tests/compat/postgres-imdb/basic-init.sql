DROP TABLE IF EXISTS person_info, movie_companies, company_name, movie_info_idx, movie_info,
  cast_info, title, name CASCADE;
CREATE TABLE name (id integer PRIMARY KEY, name text);
CREATE TABLE title (id integer PRIMARY KEY, title text, production_year integer, kind_id integer,
                    episode_of_id integer, season_nr integer);
CREATE TABLE cast_info (person_id integer, movie_id integer, role_id integer);
CREATE TABLE movie_info (movie_id integer, info text, info_type_id integer);
CREATE TABLE movie_info_idx (movie_id integer, info text, info_type_id integer);
CREATE TABLE movie_companies (company_id integer, movie_id integer, company_type_id integer);
CREATE TABLE company_name (id integer PRIMARY KEY, name text, country_code text);
CREATE TABLE person_info (person_id integer, info text, info_type_id integer);

INSERT INTO title VALUES
  (1, 'Finding Nemo', 2003, 1, NULL, NULL),
  (2, '24', 2001, 2, NULL, NULL),
  (3, 'Vendetta', 1999, 2, NULL, NULL);
INSERT INTO name VALUES
  (1, 'Aaker, Lee'), (2, 'Pfeiffer, Michelle'), (3, 'Barker, Clive'),
  (4, 'Barker, Clive'), (5, 'Silver, Joel'), (6, 'Tarantino, Quentin'),
  (7, 'Rawlings, Terry');
INSERT INTO cast_info (person_id, movie_id, role_id)
SELECT value, 1, 1 FROM generate_series(1, 18) AS value;
INSERT INTO cast_info (person_id, movie_id, role_id)
SELECT value, 1, 2 FROM generate_series(19, 23) AS value;
INSERT INTO cast_info VALUES
  (2, 1, 2), (3, 1, 4), (4, 1, 4), (5, 1, 3), (6, 1, 8), (7, 1, 9),
  (25, 1, 8), (26, 1, 3), (27, 1, 3);
INSERT INTO name (id, name)
SELECT value, 'Performer ' || value FROM generate_series(8, 27) AS value;
INSERT INTO name (id, name)
SELECT 1000000 + value, 'z-name-' || value FROM generate_series(1, 196531) AS value;
INSERT INTO movie_info VALUES
  (1, 'Action', 3), (2, 'Action', 3),
  (1, '100', 105), (2, '101', 105),
  (1, '200', 107), (1, '201', 107), (2, '202', 107),
  (3, '203', 107), (3, '204', 107);
INSERT INTO movie_info (movie_id, info, info_type_id)
SELECT 1, 'gross-' || value, 107 FROM generate_series(1, 87) AS value;
INSERT INTO movie_info (movie_id, info, info_type_id)
SELECT 2000000 + value, 'Romance', 3 FROM generate_series(1, 29368) AS value;
INSERT INTO movie_info_idx VALUES (1, '8.0', 101), (2, '7.0', 101);
INSERT INTO company_name (id, name, country_code)
SELECT 3000000 + value, 'Japan company ' || value, '[jp]'
FROM generate_series(1, 7738) AS value;
INSERT INTO title (id, title, production_year, kind_id)
SELECT 5000000 + value, 'East Asia movie ' || value,
       CASE WHEN value <= 8519 THEN 2005 ELSE 1999 END, 1
FROM generate_series(1, 15173) AS value;
INSERT INTO movie_companies (company_id, movie_id, company_type_id)
SELECT 3000000 + ((value - 1) % 7738) + 1, 5000000 + value, 2
FROM generate_series(1, 15173) AS value;
-- 这 2127 条同属东亚 production company，供原始 Action join 场景使用。
INSERT INTO movie_info (movie_id, info, info_type_id)
SELECT 5000000 + value, 'Action', 3 FROM generate_series(1, 2127) AS value;
-- 36 位演员兼任导演；与上面的 Romance 基数一起仍精确为 29405。
INSERT INTO name (id, name)
SELECT 7000000 + value, 'East Asia actor-director ' || value
FROM generate_series(1, 36) AS value;
INSERT INTO cast_info (person_id, movie_id, role_id)
SELECT 7000000 + value, 5000000 + value, 1 FROM generate_series(1, 36) AS value;
INSERT INTO cast_info (person_id, movie_id, role_id)
SELECT 7000000 + value, 5000000 + value, 8 FROM generate_series(1, 36) AS value;
INSERT INTO movie_info (movie_id, info, info_type_id)
SELECT 5000000 + value, 'Romance', 3 FROM generate_series(1, 36) AS value;
INSERT INTO company_name (id, name, country_code)
SELECT 4000000 + value, 'France company ' || value, '[fr]'
FROM generate_series(1, 19167) AS value;
INSERT INTO company_name VALUES (41, 'Studio One', '[us]'), (42, 'Studio Two', '[us]'), (43, 'Studio Three', '[us]');
INSERT INTO movie_companies VALUES (41, 1, 2), (42, 1, 2), (43, 1, 2);
