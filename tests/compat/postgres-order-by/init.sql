CREATE TABLE order_addresses (
  id integer PRIMARY KEY,
  country text NOT NULL,
  number integer NOT NULL,
  street text NOT NULL
);
INSERT INTO order_addresses (id, country, number, street) VALUES
  (993, 'Italy', 2, 'Via Roma'),
  (991, 'Italy', 1, 'Via Milano'),
  (997, 'Germany', 9, 'Straße A'),
  (992, 'Germany', 2, 'Straße Z'),
  (995, 'Germany', 1, 'Straße B');
