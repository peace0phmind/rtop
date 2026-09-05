DROP TABLE IF EXISTS company;
CREATE TABLE company (
  id integer PRIMARY KEY,
  name character varying(100),
  founding_year integer
);
INSERT INTO company VALUES
  (1, 'Big Company', 2013),
  (2, 'Some Factory', 1970);
