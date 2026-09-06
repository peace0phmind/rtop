-- 来源：Ontop lightweight-db-test-images/pgsql/sql/nested-postgresql.sql
-- 仅保留当前数据库中的两张源表；展开由 OBDA source SQL 在查询时完成。
DROP VIEW IF EXISTS company_data, company_data_arrays;
DROP TABLE IF EXISTS nested_company_data, nested_company_data_arrays;

CREATE TABLE nested_company_data (
    id integer PRIMARY KEY,
    days jsonb,
    income jsonb,
    workers jsonb,
    managers jsonb
);

INSERT INTO nested_company_data VALUES
  (1, jsonb_build_array('2023-01-01 18:00:00', '2023-01-15 18:00:00', '2023-01-29 12:00:00'), jsonb_build_array(10000, 18000, 13000), jsonb_build_array(jsonb_build_array('Sam', 'Cynthia'), jsonb_build_array('Bob'), jsonb_build_array('Jim')), '[{"firstName": "Mary", "lastName": "Jane", "age": 28}, {"firstName": "Carlos", "lastName": "Carlson", "age": 45}, {"firstName": "John", "lastName": "Moriarty", "age": 60}]'::jsonb),
  (2, jsonb_build_array('2023-02-12 18:00:00', '2023-02-26 18:00:00'), jsonb_build_array(14000, 0), jsonb_build_array(jsonb_build_array('Jim', 'Cynthia'), jsonb_build_array()), '[{"firstName": "Helena", "lastName": "of Troy"}, {"firstName": "Robert", "lastName": "Smith", "age": 48}]'::jsonb),
  (3, jsonb_build_array('2023-03-12 18:00:00', '2023-03-26 18:00:00'), jsonb_build_array(15000, 20000), jsonb_build_array(jsonb_build_array('Carl', 'Bob', 'Cynthia'), jsonb_build_array('Jim', 'Bob')), '[{"firstName": "Joseph", "lastName": "Grey"}, {"firstName": "Godfrey", "lastName": "Hamilton", "age": 59}]'::jsonb),
  (4, '[]', '[]', NULL, '[]');

CREATE TABLE nested_company_data_arrays (
    id integer PRIMARY KEY,
    days timestamp[],
    income integer[],
    workers text[][],
    managers jsonb[]
);

INSERT INTO nested_company_data_arrays VALUES
  (1, ARRAY['2023-01-01 18:00:00'::timestamp, '2023-01-15 18:00:00'::timestamp, '2023-01-29 12:00:00'::timestamp], ARRAY[10000, 18000, 13000], ARRAY[['Sam', 'Cynthia'], ['Bob', NULL], ['Jim', NULL]], ARRAY['{"firstName": "Mary", "lastName": "Jane", "age": 28}'::jsonb, '{"firstName": "Carlos", "lastName": "Carlson", "age": 45}'::jsonb, '{"firstName": "John", "lastName": "Moriarty", "age": 60}'::jsonb]),
  (2, ARRAY['2023-02-12 18:00:00'::timestamp, '2023-02-26 18:00:00'::timestamp], ARRAY[14000, 0], ARRAY[['Jim', 'Cynthia'], [NULL, NULL]], ARRAY['{"firstName": "Helena", "lastName": "of Troy"}'::jsonb, '{"firstName": "Robert", "lastName": "Smith", "age": 48}'::jsonb]),
  (3, ARRAY['2023-03-12 18:00:00'::timestamp, '2023-03-26 18:00:00'::timestamp], ARRAY[15000, 20000], ARRAY[['Carl', 'Bob', 'Cynthia'], ['Jim', 'Bob', NULL]], ARRAY['{"firstName": "Joseph", "lastName": "Grey"}'::jsonb, '{"firstName": "Godfrey", "lastName": "Hamilton", "age": 59}'::jsonb]),
  (4, ARRAY[]::timestamp[], ARRAY[]::integer[], NULL, ARRAY[]::jsonb[]);

-- 固定 Ontop NestedData PostgreSQL fixture 的 lenses 使用这些原始 relation
-- 名称；以视图把同一物理行同时提供给 Java baseline 和 Rust native-OBDA。
CREATE VIEW company_data AS SELECT * FROM nested_company_data;
CREATE VIEW company_data_arrays AS SELECT * FROM nested_company_data_arrays;
