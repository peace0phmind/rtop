CREATE TABLE "Book" (
  bk_code integer NOT NULL,
  bk_title character varying(100),
  CONSTRAINT pk_book PRIMARY KEY (bk_code)
);

CREATE TABLE "Writer" (
  wr_code integer NOT NULL,
  wr_name character varying(100),
  CONSTRAINT pk_writer PRIMARY KEY (wr_code)
);

CREATE TABLE "BookWriter" (
  bk_code integer,
  wr_code integer,
  CONSTRAINT fk_book_writer FOREIGN KEY (bk_code) REFERENCES "Book"(bk_code),
  CONSTRAINT fk_writer_book FOREIGN KEY (wr_code) REFERENCES "Writer"(wr_code)
);

CREATE TABLE "Edition" (
  ed_code integer NOT NULL,
  ed_year integer,
  bk_code integer,
  CONSTRAINT pk_edition PRIMARY KEY (ed_code),
  CONSTRAINT fk_book_edition FOREIGN KEY (bk_code) REFERENCES "Book"(bk_code)
);
