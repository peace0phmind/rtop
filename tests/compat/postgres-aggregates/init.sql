-- 固定 Ontop distinctInAggregates/university.obda 的 PostgreSQL 输入。
CREATE TABLE course (
  course_id varchar(100) PRIMARY KEY,
  nb_students integer NOT NULL,
  duration numeric(38,4) NOT NULL
);
CREATE TABLE teaching (
  course_id varchar(100) NOT NULL,
  prof_id integer NOT NULL,
  PRIMARY KEY (course_id, prof_id)
);
CREATE TABLE professors (
  prof_id integer PRIMARY KEY,
  first_name varchar(100) NOT NULL,
  last_name varchar(100) NOT NULL,
  nickname varchar(100)
);
INSERT INTO course (course_id, nb_students, duration) VALUES
  ('LinearAlgebra', 10, 24.5),
  ('DiscreteMathematics', 11, 30),
  ('AdvancedDatabases', 12, 20),
  ('ScientificWriting', 13, 18),
  ('OperatingSystems', 10, 30);
INSERT INTO teaching (course_id, prof_id) VALUES
  ('LinearAlgebra', 1),
  ('DiscreteMathematics', 1),
  ('AdvancedDatabases', 3),
  ('ScientificWriting', 8),
  ('OperatingSystems', 1);
INSERT INTO professors (prof_id, first_name, last_name, nickname) VALUES
  (1, 'Roger', 'Smith', 'Rog'),
  (2, 'Frank', 'Pitt', 'Frankie'),
  (3, 'John', 'Depp', 'Johnny'),
  (4, 'Michael', 'Jackson', 'King of Pop'),
  (5, 'Diego', 'Gamper', NULL),
  (6, 'Johann', 'Helmer', NULL),
  (7, 'Barbara', 'Dodero', NULL),
  (8, 'Mary', 'Poppins', NULL);
