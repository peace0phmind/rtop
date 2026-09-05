CREATE TABLE cast_info (person_id integer, movie_id integer, role_id integer);
CREATE TABLE IF NOT EXISTS title (id integer PRIMARY KEY, title text, production_year integer, kind_id integer);
ALTER TABLE title ADD COLUMN IF NOT EXISTS kind_id integer;
CREATE TABLE movie_companies (company_id integer, company_type_id integer);
CREATE TABLE person_info (person_id integer, info text, info_type_id integer);
CREATE TABLE IF NOT EXISTS name (id integer PRIMARY KEY, name text);
CREATE TABLE movie_info_idx (movie_id integer, info text, info_type_id integer);
CREATE TABLE movie_info (movie_id integer, info text, info_type_id integer);
INSERT INTO cast_info VALUES (10, 1, 1);
INSERT INTO title (id, title, production_year, kind_id) VALUES (1, 'A title', 2020, 1)
ON CONFLICT (id) DO UPDATE SET title = EXCLUDED.title, production_year = EXCLUDED.production_year, kind_id = EXCLUDED.kind_id;
INSERT INTO movie_companies VALUES (7, 2);
INSERT INTO person_info VALUES (10, '2000-01-01', 21);
INSERT INTO name (id, name) VALUES (10, 'Ada')
ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name;
INSERT INTO movie_info_idx VALUES (1, '7.5', 101);
INSERT INTO movie_info VALUES (1, 'genre', 3), (1, '100', 105), (1, '200', 107);
