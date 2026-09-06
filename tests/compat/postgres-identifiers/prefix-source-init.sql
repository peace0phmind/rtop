CREATE TABLE name (id integer PRIMARY KEY, name text);
CREATE TABLE title (id integer PRIMARY KEY, title text, production_year integer, kind_id integer);
CREATE TABLE cast_info (person_id integer, movie_id integer, role_id integer);
CREATE TABLE movie_info (movie_id integer, info text, info_type_id integer);
CREATE TABLE movie_info_idx (movie_id integer, info text, info_type_id integer);
CREATE TABLE movie_companies (company_id integer, movie_id integer, company_type_id integer);
CREATE TABLE company_name (id integer PRIMARY KEY, name text, country_code text);
CREATE TABLE person_info (person_id integer, info text, info_type_id integer);

INSERT INTO company_name VALUES (7, 'Japan Company', '[jp]');
