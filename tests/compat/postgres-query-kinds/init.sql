CREATE TABLE query_people (id integer PRIMARY KEY);
INSERT INTO query_people (id) VALUES (1);
CREATE TABLE query_named_people (id integer PRIMARY KEY, name text NOT NULL);
INSERT INTO query_named_people (id, name) VALUES (1, 'Ada');
CREATE TABLE query_nullable_people (id integer PRIMARY KEY, name text);
INSERT INTO query_nullable_people (id, name) VALUES (1, 'Ada'), (2, NULL);
