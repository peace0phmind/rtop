CREATE TABLE cast_info (person_id integer, movie_id integer, role_id integer);
CREATE TABLE name (id integer PRIMARY KEY, name text NOT NULL);
CREATE TABLE title (id integer PRIMARY KEY, title text NOT NULL, production_year integer NOT NULL);
INSERT INTO cast_info VALUES (222, 97263, 1);
INSERT INTO name VALUES (222, 'A.J.');
INSERT INTO title VALUES (97263, 'Colleen', 2001);
