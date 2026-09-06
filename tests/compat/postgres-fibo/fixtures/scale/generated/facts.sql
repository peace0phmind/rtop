\connect fibo_scale

CREATE TABLE scale_load_audit (started_at timestamptz NOT NULL, finished_at timestamptz);

INSERT INTO scale_load_audit VALUES (clock_timestamp(), NULL);

CREATE EXTENSION IF NOT EXISTS "btree_gist";

CREATE TABLE "region" (
  "region_id" text NOT NULL,
  "label" text NOT NULL,
  PRIMARY KEY ("region_id")
);

CREATE TABLE "booking_institution" (
  "institution_id" text NOT NULL,
  "label" text NOT NULL,
  "region_id" text NOT NULL,
  PRIMARY KEY ("institution_id"),
  FOREIGN KEY ("region_id") REFERENCES region(region_id)
);

CREATE TABLE "lender" (
  "lender_id" text NOT NULL,
  "label" text NOT NULL,
  PRIMARY KEY ("lender_id")
);

CREATE TABLE "loan_contract" (
  "loan_id" text NOT NULL,
  "booking_institution_id" text NOT NULL,
  "lender_id" text NOT NULL,
  PRIMARY KEY ("loan_id"),
  FOREIGN KEY ("booking_institution_id") REFERENCES booking_institution(institution_id),
  FOREIGN KEY ("lender_id") REFERENCES lender(lender_id)
);

CREATE TABLE "balance_observation" (
  "fact_id" text NOT NULL,
  "loan_id" text NOT NULL,
  "amount" numeric(18,4) NOT NULL,
  "currency" char(3) NOT NULL,
  "valid_from" date NOT NULL,
  "valid_to" date,
  PRIMARY KEY ("fact_id"),
  UNIQUE ("loan_id", "valid_from"),
  FOREIGN KEY ("loan_id") REFERENCES loan_contract(loan_id),
  CHECK (amount >= 0),
  CHECK (currency = 'CNY'),
  CHECK (valid_to IS NULL OR valid_from < valid_to),
  EXCLUDE USING gist (loan_id WITH =, daterange(valid_from, valid_to, '[)') WITH &&)
);

CREATE TABLE "disbursement_event" (
  "event_id" text NOT NULL,
  "loan_id" text NOT NULL,
  "amount" numeric(18,4) NOT NULL,
  "currency" char(3) NOT NULL,
  "occurred_at" date NOT NULL,
  PRIMARY KEY ("event_id"),
  FOREIGN KEY ("loan_id") REFERENCES loan_contract(loan_id),
  CHECK (amount >= 0),
  CHECK (currency = 'CNY')
);

CREATE TABLE "region_alias" (
  "region_id" text NOT NULL,
  "alias" text NOT NULL,
  PRIMARY KEY ("region_id", "alias"),
  FOREIGN KEY ("region_id") REFERENCES region(region_id)
);

CREATE TABLE "source_lender_description" (
  "description_id" text NOT NULL,
  "source_system" text NOT NULL,
  "source_id" text NOT NULL,
  "display_label" text NOT NULL,
  "tax_id" text,
  PRIMARY KEY ("description_id"),
  UNIQUE ("source_system", "source_id")
);

CREATE TABLE "identity_dataset_version" (
  "dataset_version" text NOT NULL,
  "parent_version" text,
  "published_at" date NOT NULL,
  "sealed" boolean NOT NULL,
  PRIMARY KEY ("dataset_version"),
  CHECK (sealed)
);

CREATE TABLE "identity_decision" (
  "dataset_version" text NOT NULL,
  "description_id" text NOT NULL,
  "canonical_lender_id" text NOT NULL,
  "evidence" text NOT NULL,
  "decision_status" text NOT NULL,
  PRIMARY KEY ("dataset_version", "description_id"),
  FOREIGN KEY ("dataset_version") REFERENCES identity_dataset_version(dataset_version),
  FOREIGN KEY ("description_id") REFERENCES source_lender_description(description_id),
  FOREIGN KEY ("canonical_lender_id") REFERENCES lender(lender_id),
  CHECK (decision_status IN ('accepted', 'rejected', 'pending'))
);

CREATE TABLE "party" (
  "party_id" text NOT NULL,
  "label" text NOT NULL,
  "region_id" text NOT NULL,
  PRIMARY KEY ("party_id"),
  FOREIGN KEY ("region_id") REFERENCES region(region_id)
);

CREATE TABLE "loan_participation" (
  "participation_id" text NOT NULL,
  "loan_id" text NOT NULL,
  "party_id" text NOT NULL,
  "role" text NOT NULL,
  "valid_from" date NOT NULL,
  "valid_to" date,
  "allocation_weight" numeric(8,6),
  "allocation_policy" text,
  "legal_share" numeric(8,6),
  PRIMARY KEY ("participation_id"),
  UNIQUE ("loan_id", "party_id", "role", "valid_from"),
  FOREIGN KEY ("loan_id") REFERENCES loan_contract(loan_id),
  FOREIGN KEY ("party_id") REFERENCES party(party_id),
  CHECK (valid_to IS NULL OR valid_from < valid_to),
  CHECK (allocation_weight IS NULL OR (allocation_weight >= 0 AND allocation_weight <= 1)),
  CHECK (legal_share IS NULL OR (legal_share >= 0 AND legal_share <= 1)),
  EXCLUDE USING gist (loan_id WITH =, daterange(valid_from, valid_to, '[)') WITH &&) WHERE (role = 'primary-borrower')
);

INSERT INTO "region" VALUES ('scale.fictional.region.east', '虚构华东');

INSERT INTO "region" VALUES ('scale.fictional.region.south', '虚构华南');

INSERT INTO "booking_institution" VALUES ('scale.fictional.institution.east', '虚构华东机构', 'scale.fictional.region.east');

INSERT INTO "booking_institution" VALUES ('scale.fictional.institution.south', '虚构华南机构', 'scale.fictional.region.south');

INSERT INTO "lender" VALUES ('scale.fictional.lender.000', '虚构贷款人000');

INSERT INTO "lender" VALUES ('scale.fictional.lender.001', '虚构贷款人001');

INSERT INTO "lender" VALUES ('scale.fictional.lender.002', '虚构贷款人002');

INSERT INTO "lender" VALUES ('scale.fictional.lender.003', '虚构贷款人003');

INSERT INTO "lender" VALUES ('scale.fictional.lender.004', '虚构贷款人004');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000000', 'scale.fictional.institution.east', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000000.v1', 'scale.fictional.loan.000000', '10.0000', 'CNY', '2025-01-01', '2025-07-01');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000000.current', 'scale.fictional.loan.000000', '0.0000', 'CNY', '2025-07-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000000', 'scale.fictional.loan.000000', '0.0000', 'CNY', '2025-01-01');

INSERT INTO "party" VALUES ('scale.fictional.party.000000.primary', '虚构参与方000000主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000000.primary', 'scale.fictional.loan.000000', 'scale.fictional.party.000000.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000000.co', '虚构参与方000000共', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000000.co', 'scale.fictional.loan.000000', 'scale.fictional.party.000000.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000001', 'scale.fictional.institution.east', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000001.current', 'scale.fictional.loan.000001', '2222.0000', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000001', 'scale.fictional.loan.000001', '26.0000', 'CNY', '2025-01-04');

INSERT INTO "party" VALUES ('scale.fictional.party.000001.primary', '虚构参与方000001主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000001.primary', 'scale.fictional.loan.000001', 'scale.fictional.party.000001.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000001.co', '虚构参与方000001共', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000001.co', 'scale.fictional.loan.000001', 'scale.fictional.party.000001.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000002', 'scale.fictional.institution.east', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000002.current', 'scale.fictional.loan.000002', '2222.0000', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000002', 'scale.fictional.loan.000002', '39.0000', 'CNY', '2025-01-08');

INSERT INTO "party" VALUES ('scale.fictional.party.000002.primary', '虚构参与方000002主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000002.primary', 'scale.fictional.loan.000002', 'scale.fictional.party.000002.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000002.co', '虚构参与方000002共', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000002.co', 'scale.fictional.loan.000002', 'scale.fictional.party.000002.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000003', 'scale.fictional.institution.east', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000003.current', 'scale.fictional.loan.000003', '11100.0900', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000003', 'scale.fictional.loan.000003', '52.0000', 'CNY', '2025-01-11');

INSERT INTO "party" VALUES ('scale.fictional.party.000003.primary', '虚构参与方000003主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000003.primary', 'scale.fictional.loan.000003', 'scale.fictional.party.000003.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000004', 'scale.fictional.institution.east', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000004.current', 'scale.fictional.loan.000004', '14800.1200', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000004', 'scale.fictional.loan.000004', '65.0000', 'CNY', '2025-01-15');

INSERT INTO "party" VALUES ('scale.fictional.party.000004.primary', '虚构参与方000004主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000004.primary', 'scale.fictional.loan.000004', 'scale.fictional.party.000004.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000004.co', '虚构参与方000004共', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000004.co', 'scale.fictional.loan.000004', 'scale.fictional.party.000004.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000005', 'scale.fictional.institution.east', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000005.current', 'scale.fictional.loan.000005', '18500.1500', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000005', 'scale.fictional.loan.000005', '78.0000', 'CNY', '2025-01-19');

INSERT INTO "party" VALUES ('scale.fictional.party.000005.primary', '虚构参与方000005主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000005.primary', 'scale.fictional.loan.000005', 'scale.fictional.party.000005.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000006', 'scale.fictional.institution.east', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000006.current', 'scale.fictional.loan.000006', '22200.1800', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000006', 'scale.fictional.loan.000006', '91.0000', 'CNY', '2025-01-22');

INSERT INTO "party" VALUES ('scale.fictional.party.000006.primary', '虚构参与方000006主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000006.primary', 'scale.fictional.loan.000006', 'scale.fictional.party.000006.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000006.co', '虚构参与方000006共', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000006.co', 'scale.fictional.loan.000006', 'scale.fictional.party.000006.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000007', 'scale.fictional.institution.east', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000007.current', 'scale.fictional.loan.000007', '25900.2100', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000007', 'scale.fictional.loan.000007', '104.0000', 'CNY', '2025-01-26');

INSERT INTO "party" VALUES ('scale.fictional.party.000007.primary', '虚构参与方000007主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000007.primary', 'scale.fictional.loan.000007', 'scale.fictional.party.000007.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000008', 'scale.fictional.institution.east', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000008.current', 'scale.fictional.loan.000008', '29600.2400', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000008', 'scale.fictional.loan.000008', '117.0000', 'CNY', '2025-01-30');

INSERT INTO "party" VALUES ('scale.fictional.party.000008.primary', '虚构参与方000008主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000008.primary', 'scale.fictional.loan.000008', 'scale.fictional.party.000008.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000009', 'scale.fictional.institution.east', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000009.current', 'scale.fictional.loan.000009', '33300.2700', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000009', 'scale.fictional.loan.000009', '130.0000', 'CNY', '2025-02-02');

INSERT INTO "party" VALUES ('scale.fictional.party.000009.primary', '虚构参与方000009主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000009.primary', 'scale.fictional.loan.000009', 'scale.fictional.party.000009.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000010', 'scale.fictional.institution.east', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000010.current', 'scale.fictional.loan.000010', '37000.3000', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000010', 'scale.fictional.loan.000010', '143.0000', 'CNY', '2025-02-06');

INSERT INTO "party" VALUES ('scale.fictional.party.000010.primary', '虚构参与方000010主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000010.primary', 'scale.fictional.loan.000010', 'scale.fictional.party.000010.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000011', 'scale.fictional.institution.east', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000011.current', 'scale.fictional.loan.000011', '40700.3300', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000011', 'scale.fictional.loan.000011', '156.0000', 'CNY', '2025-02-10');

INSERT INTO "party" VALUES ('scale.fictional.party.000011.primary', '虚构参与方000011主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000011.primary', 'scale.fictional.loan.000011', 'scale.fictional.party.000011.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000012', 'scale.fictional.institution.east', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000012.current', 'scale.fictional.loan.000012', '44400.3600', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000012', 'scale.fictional.loan.000012', '169.0000', 'CNY', '2025-02-13');

INSERT INTO "party" VALUES ('scale.fictional.party.000012.primary', '虚构参与方000012主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000012.primary', 'scale.fictional.loan.000012', 'scale.fictional.party.000012.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000012.co', '虚构参与方000012共', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000012.co', 'scale.fictional.loan.000012', 'scale.fictional.party.000012.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000013', 'scale.fictional.institution.east', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000013.v1', 'scale.fictional.loan.000013', '48110.3900', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000013.current', 'scale.fictional.loan.000013', '48100.3900', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000013', 'scale.fictional.loan.000013', '182.0000', 'CNY', '2025-02-17');

INSERT INTO "party" VALUES ('scale.fictional.party.000013.primary', '虚构参与方000013主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000013.primary', 'scale.fictional.loan.000013', 'scale.fictional.party.000013.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000014', 'scale.fictional.institution.east', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000014.v1', 'scale.fictional.loan.000014', '51810.4200', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000014.current', 'scale.fictional.loan.000014', '51800.4200', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000014', 'scale.fictional.loan.000014', '195.0000', 'CNY', '2025-02-21');

INSERT INTO "party" VALUES ('scale.fictional.party.000014.primary', '虚构参与方000014主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000014.primary', 'scale.fictional.loan.000014', 'scale.fictional.party.000014.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000015', 'scale.fictional.institution.east', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000015.current', 'scale.fictional.loan.000015', '55500.4500', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000015', 'scale.fictional.loan.000015', '208.0000', 'CNY', '2025-02-24');

INSERT INTO "party" VALUES ('scale.fictional.party.000015.primary', '虚构参与方000015主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000015.primary', 'scale.fictional.loan.000015', 'scale.fictional.party.000015.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000016', 'scale.fictional.institution.east', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000016.current', 'scale.fictional.loan.000016', '59200.4800', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000016', 'scale.fictional.loan.000016', '221.0000', 'CNY', '2025-02-28');

INSERT INTO "party" VALUES ('scale.fictional.party.000016.primary', '虚构参与方000016主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000016.primary', 'scale.fictional.loan.000016', 'scale.fictional.party.000016.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000016.co', '虚构参与方000016共', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000016.co', 'scale.fictional.loan.000016', 'scale.fictional.party.000016.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000017', 'scale.fictional.institution.east', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000017.current', 'scale.fictional.loan.000017', '62900.5100', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000017', 'scale.fictional.loan.000017', '234.0000', 'CNY', '2025-03-04');

INSERT INTO "party" VALUES ('scale.fictional.party.000017.primary', '虚构参与方000017主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000017.primary', 'scale.fictional.loan.000017', 'scale.fictional.party.000017.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000018', 'scale.fictional.institution.east', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000018.v1', 'scale.fictional.loan.000018', '66610.5400', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000018.current', 'scale.fictional.loan.000018', '66600.5400', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000018', 'scale.fictional.loan.000018', '247.0000', 'CNY', '2025-03-07');

INSERT INTO "party" VALUES ('scale.fictional.party.000018.primary', '虚构参与方000018主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000018.primary', 'scale.fictional.loan.000018', 'scale.fictional.party.000018.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000018.co', '虚构参与方000018共', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000018.co', 'scale.fictional.loan.000018', 'scale.fictional.party.000018.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000019', 'scale.fictional.institution.east', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000019.v1', 'scale.fictional.loan.000019', '70310.5700', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000019.current', 'scale.fictional.loan.000019', '70300.5700', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000019', 'scale.fictional.loan.000019', '260.0000', 'CNY', '2025-03-11');

INSERT INTO "party" VALUES ('scale.fictional.party.000019.primary', '虚构参与方000019主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000019.primary', 'scale.fictional.loan.000019', 'scale.fictional.party.000019.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000020', 'scale.fictional.institution.east', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000020.current', 'scale.fictional.loan.000020', '74000.6000', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000020', 'scale.fictional.loan.000020', '273.0000', 'CNY', '2025-03-15');

INSERT INTO "party" VALUES ('scale.fictional.party.000020.primary', '虚构参与方000020主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000020.primary', 'scale.fictional.loan.000020', 'scale.fictional.party.000020.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000021', 'scale.fictional.institution.east', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000021.current', 'scale.fictional.loan.000021', '77700.6300', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000021', 'scale.fictional.loan.000021', '286.0000', 'CNY', '2025-03-18');

INSERT INTO "party" VALUES ('scale.fictional.party.000021.primary', '虚构参与方000021主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000021.primary', 'scale.fictional.loan.000021', 'scale.fictional.party.000021.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000022', 'scale.fictional.institution.east', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000022.current', 'scale.fictional.loan.000022', '81400.6600', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000022', 'scale.fictional.loan.000022', '299.0000', 'CNY', '2025-03-22');

INSERT INTO "party" VALUES ('scale.fictional.party.000022.primary', '虚构参与方000022主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000022.primary', 'scale.fictional.loan.000022', 'scale.fictional.party.000022.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000023', 'scale.fictional.institution.east', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000023.current', 'scale.fictional.loan.000023', '85100.6900', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000023', 'scale.fictional.loan.000023', '312.0000', 'CNY', '2025-03-25');

INSERT INTO "party" VALUES ('scale.fictional.party.000023.primary', '虚构参与方000023主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000023.primary', 'scale.fictional.loan.000023', 'scale.fictional.party.000023.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000024', 'scale.fictional.institution.east', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000024.v1', 'scale.fictional.loan.000024', '88810.7200', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000024.current', 'scale.fictional.loan.000024', '88800.7200', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000024', 'scale.fictional.loan.000024', '325.0000', 'CNY', '2025-03-29');

INSERT INTO "party" VALUES ('scale.fictional.party.000024.primary', '虚构参与方000024主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000024.primary', 'scale.fictional.loan.000024', 'scale.fictional.party.000024.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000025', 'scale.fictional.institution.east', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000025.current', 'scale.fictional.loan.000025', '92500.7500', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000025', 'scale.fictional.loan.000025', '338.0000', 'CNY', '2025-04-02');

INSERT INTO "party" VALUES ('scale.fictional.party.000025.primary', '虚构参与方000025主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000025.primary', 'scale.fictional.loan.000025', 'scale.fictional.party.000025.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000026', 'scale.fictional.institution.east', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000026.current', 'scale.fictional.loan.000026', '96200.7800', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000026', 'scale.fictional.loan.000026', '351.0000', 'CNY', '2025-04-05');

INSERT INTO "party" VALUES ('scale.fictional.party.000026.primary', '虚构参与方000026主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000026.primary', 'scale.fictional.loan.000026', 'scale.fictional.party.000026.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000027', 'scale.fictional.institution.east', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000027.current', 'scale.fictional.loan.000027', '99900.8100', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000027', 'scale.fictional.loan.000027', '364.0000', 'CNY', '2025-04-09');

INSERT INTO "party" VALUES ('scale.fictional.party.000027.primary', '虚构参与方000027主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000027.primary', 'scale.fictional.loan.000027', 'scale.fictional.party.000027.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000028', 'scale.fictional.institution.east', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000028.current', 'scale.fictional.loan.000028', '103600.8400', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000028', 'scale.fictional.loan.000028', '377.0000', 'CNY', '2025-04-13');

INSERT INTO "party" VALUES ('scale.fictional.party.000028.primary', '虚构参与方000028主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000028.primary', 'scale.fictional.loan.000028', 'scale.fictional.party.000028.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000029', 'scale.fictional.institution.east', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000029.current', 'scale.fictional.loan.000029', '107300.8700', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000029', 'scale.fictional.loan.000029', '390.0000', 'CNY', '2025-04-16');

INSERT INTO "party" VALUES ('scale.fictional.party.000029.primary', '虚构参与方000029主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000029.primary', 'scale.fictional.loan.000029', 'scale.fictional.party.000029.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000030', 'scale.fictional.institution.east', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000030.current', 'scale.fictional.loan.000030', '111000.9000', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000030', 'scale.fictional.loan.000030', '403.0000', 'CNY', '2025-04-20');

INSERT INTO "party" VALUES ('scale.fictional.party.000030.primary', '虚构参与方000030主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000030.primary', 'scale.fictional.loan.000030', 'scale.fictional.party.000030.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000031', 'scale.fictional.institution.east', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000031.current', 'scale.fictional.loan.000031', '114700.9300', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000031', 'scale.fictional.loan.000031', '416.0000', 'CNY', '2025-04-24');

INSERT INTO "party" VALUES ('scale.fictional.party.000031.primary', '虚构参与方000031主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000031.primary', 'scale.fictional.loan.000031', 'scale.fictional.party.000031.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000032', 'scale.fictional.institution.east', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000032.current', 'scale.fictional.loan.000032', '118400.9600', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000032', 'scale.fictional.loan.000032', '429.0000', 'CNY', '2025-04-27');

INSERT INTO "party" VALUES ('scale.fictional.party.000032.primary', '虚构参与方000032主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000032.primary', 'scale.fictional.loan.000032', 'scale.fictional.party.000032.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000033', 'scale.fictional.institution.east', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000033.current', 'scale.fictional.loan.000033', '122100.9900', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000033', 'scale.fictional.loan.000033', '442.0000', 'CNY', '2025-05-01');

INSERT INTO "party" VALUES ('scale.fictional.party.000033.primary', '虚构参与方000033主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000033.primary', 'scale.fictional.loan.000033', 'scale.fictional.party.000033.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000034', 'scale.fictional.institution.east', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000034.v1', 'scale.fictional.loan.000034', '125811.0200', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000034.current', 'scale.fictional.loan.000034', '125801.0200', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000034', 'scale.fictional.loan.000034', '455.0000', 'CNY', '2025-05-05');

INSERT INTO "party" VALUES ('scale.fictional.party.000034.primary', '虚构参与方000034主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000034.primary', 'scale.fictional.loan.000034', 'scale.fictional.party.000034.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000035', 'scale.fictional.institution.east', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000035.current', 'scale.fictional.loan.000035', '129501.0500', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000035', 'scale.fictional.loan.000035', '468.0000', 'CNY', '2025-05-08');

INSERT INTO "party" VALUES ('scale.fictional.party.000035.primary', '虚构参与方000035主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000035.primary', 'scale.fictional.loan.000035', 'scale.fictional.party.000035.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000035.co', '虚构参与方000035共', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000035.co', 'scale.fictional.loan.000035', 'scale.fictional.party.000035.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000036', 'scale.fictional.institution.east', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000036.current', 'scale.fictional.loan.000036', '133201.0800', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000036', 'scale.fictional.loan.000036', '481.0000', 'CNY', '2025-05-12');

INSERT INTO "party" VALUES ('scale.fictional.party.000036.primary', '虚构参与方000036主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000036.primary', 'scale.fictional.loan.000036', 'scale.fictional.party.000036.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000037', 'scale.fictional.institution.east', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000037.current', 'scale.fictional.loan.000037', '136901.1100', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000037', 'scale.fictional.loan.000037', '494.0000', 'CNY', '2025-05-16');

INSERT INTO "party" VALUES ('scale.fictional.party.000037.primary', '虚构参与方000037主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000037.primary', 'scale.fictional.loan.000037', 'scale.fictional.party.000037.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000038', 'scale.fictional.institution.east', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000038.v1', 'scale.fictional.loan.000038', '140611.1400', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000038.current', 'scale.fictional.loan.000038', '140601.1400', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000038', 'scale.fictional.loan.000038', '507.0000', 'CNY', '2025-05-19');

INSERT INTO "party" VALUES ('scale.fictional.party.000038.primary', '虚构参与方000038主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000038.primary', 'scale.fictional.loan.000038', 'scale.fictional.party.000038.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000039', 'scale.fictional.institution.east', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000039.current', 'scale.fictional.loan.000039', '144301.1700', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000039', 'scale.fictional.loan.000039', '520.0000', 'CNY', '2025-05-23');

INSERT INTO "party" VALUES ('scale.fictional.party.000039.primary', '虚构参与方000039主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000039.primary', 'scale.fictional.loan.000039', 'scale.fictional.party.000039.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000039.co', '虚构参与方000039共', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000039.co', 'scale.fictional.loan.000039', 'scale.fictional.party.000039.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000040', 'scale.fictional.institution.east', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000040.current', 'scale.fictional.loan.000040', '148001.2000', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000040', 'scale.fictional.loan.000040', '533.0000', 'CNY', '2025-05-27');

INSERT INTO "party" VALUES ('scale.fictional.party.000040.primary', '虚构参与方000040主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000040.primary', 'scale.fictional.loan.000040', 'scale.fictional.party.000040.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000041', 'scale.fictional.institution.east', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000041.v1', 'scale.fictional.loan.000041', '151711.2300', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000041.current', 'scale.fictional.loan.000041', '151701.2300', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000041', 'scale.fictional.loan.000041', '546.0000', 'CNY', '2025-05-30');

INSERT INTO "party" VALUES ('scale.fictional.party.000041.primary', '虚构参与方000041主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000041.primary', 'scale.fictional.loan.000041', 'scale.fictional.party.000041.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000042', 'scale.fictional.institution.east', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000042.v1', 'scale.fictional.loan.000042', '155411.2600', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000042.current', 'scale.fictional.loan.000042', '155401.2600', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000042', 'scale.fictional.loan.000042', '559.0000', 'CNY', '2025-06-03');

INSERT INTO "party" VALUES ('scale.fictional.party.000042.primary', '虚构参与方000042主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000042.primary', 'scale.fictional.loan.000042', 'scale.fictional.party.000042.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000042.co', '虚构参与方000042共', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000042.co', 'scale.fictional.loan.000042', 'scale.fictional.party.000042.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000043', 'scale.fictional.institution.east', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000043.current', 'scale.fictional.loan.000043', '159101.2900', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000043', 'scale.fictional.loan.000043', '572.0000', 'CNY', '2025-06-06');

INSERT INTO "party" VALUES ('scale.fictional.party.000043.primary', '虚构参与方000043主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000043.primary', 'scale.fictional.loan.000043', 'scale.fictional.party.000043.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000044', 'scale.fictional.institution.east', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000044.current', 'scale.fictional.loan.000044', '162801.3200', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000044', 'scale.fictional.loan.000044', '585.0000', 'CNY', '2025-06-10');

INSERT INTO "party" VALUES ('scale.fictional.party.000044.primary', '虚构参与方000044主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000044.primary', 'scale.fictional.loan.000044', 'scale.fictional.party.000044.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000045', 'scale.fictional.institution.east', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000045.current', 'scale.fictional.loan.000045', '166501.3500', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000045', 'scale.fictional.loan.000045', '598.0000', 'CNY', '2025-06-14');

INSERT INTO "party" VALUES ('scale.fictional.party.000045.primary', '虚构参与方000045主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000045.primary', 'scale.fictional.loan.000045', 'scale.fictional.party.000045.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000046', 'scale.fictional.institution.east', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000046.current', 'scale.fictional.loan.000046', '170201.3800', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000046', 'scale.fictional.loan.000046', '611.0000', 'CNY', '2025-06-17');

INSERT INTO "party" VALUES ('scale.fictional.party.000046.primary', '虚构参与方000046主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000046.primary', 'scale.fictional.loan.000046', 'scale.fictional.party.000046.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000047', 'scale.fictional.institution.east', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000047.current', 'scale.fictional.loan.000047', '173901.4100', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000047', 'scale.fictional.loan.000047', '624.0000', 'CNY', '2025-06-21');

INSERT INTO "party" VALUES ('scale.fictional.party.000047.primary', '虚构参与方000047主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000047.primary', 'scale.fictional.loan.000047', 'scale.fictional.party.000047.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000047.co', '虚构参与方000047共', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000047.co', 'scale.fictional.loan.000047', 'scale.fictional.party.000047.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000048', 'scale.fictional.institution.east', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000048.current', 'scale.fictional.loan.000048', '177601.4400', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000048', 'scale.fictional.loan.000048', '637.0000', 'CNY', '2025-06-25');

INSERT INTO "party" VALUES ('scale.fictional.party.000048.primary', '虚构参与方000048主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000048.primary', 'scale.fictional.loan.000048', 'scale.fictional.party.000048.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000049', 'scale.fictional.institution.east', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000049.current', 'scale.fictional.loan.000049', '181301.4700', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000049', 'scale.fictional.loan.000049', '650.0000', 'CNY', '2025-06-28');

INSERT INTO "party" VALUES ('scale.fictional.party.000049.primary', '虚构参与方000049主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000049.primary', 'scale.fictional.loan.000049', 'scale.fictional.party.000049.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000050', 'scale.fictional.institution.east', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000050.v1', 'scale.fictional.loan.000050', '185011.5000', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000050.current', 'scale.fictional.loan.000050', '185001.5000', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000050', 'scale.fictional.loan.000050', '663.0000', 'CNY', '2025-07-02');

INSERT INTO "party" VALUES ('scale.fictional.party.000050.primary', '虚构参与方000050主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000050.primary', 'scale.fictional.loan.000050', 'scale.fictional.party.000050.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000051', 'scale.fictional.institution.east', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000051.current', 'scale.fictional.loan.000051', '188701.5300', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000051', 'scale.fictional.loan.000051', '676.0000', 'CNY', '2025-07-06');

INSERT INTO "party" VALUES ('scale.fictional.party.000051.primary', '虚构参与方000051主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000051.primary', 'scale.fictional.loan.000051', 'scale.fictional.party.000051.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000052', 'scale.fictional.institution.east', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000052.current', 'scale.fictional.loan.000052', '192401.5600', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000052', 'scale.fictional.loan.000052', '689.0000', 'CNY', '2025-07-09');

INSERT INTO "party" VALUES ('scale.fictional.party.000052.primary', '虚构参与方000052主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000052.primary', 'scale.fictional.loan.000052', 'scale.fictional.party.000052.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000052.co', '虚构参与方000052共', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000052.co', 'scale.fictional.loan.000052', 'scale.fictional.party.000052.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000053', 'scale.fictional.institution.east', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000053.current', 'scale.fictional.loan.000053', '196101.5900', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000053', 'scale.fictional.loan.000053', '702.0000', 'CNY', '2025-07-13');

INSERT INTO "party" VALUES ('scale.fictional.party.000053.primary', '虚构参与方000053主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000053.primary', 'scale.fictional.loan.000053', 'scale.fictional.party.000053.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000054', 'scale.fictional.institution.east', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000054.current', 'scale.fictional.loan.000054', '199801.6200', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000054', 'scale.fictional.loan.000054', '715.0000', 'CNY', '2025-07-17');

INSERT INTO "party" VALUES ('scale.fictional.party.000054.primary', '虚构参与方000054主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000054.primary', 'scale.fictional.loan.000054', 'scale.fictional.party.000054.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000055', 'scale.fictional.institution.east', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000055.current', 'scale.fictional.loan.000055', '203501.6500', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000055', 'scale.fictional.loan.000055', '728.0000', 'CNY', '2025-07-20');

INSERT INTO "party" VALUES ('scale.fictional.party.000055.primary', '虚构参与方000055主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000055.primary', 'scale.fictional.loan.000055', 'scale.fictional.party.000055.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000056', 'scale.fictional.institution.east', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000056.current', 'scale.fictional.loan.000056', '207201.6800', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000056', 'scale.fictional.loan.000056', '741.0000', 'CNY', '2025-07-24');

INSERT INTO "party" VALUES ('scale.fictional.party.000056.primary', '虚构参与方000056主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000056.primary', 'scale.fictional.loan.000056', 'scale.fictional.party.000056.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000057', 'scale.fictional.institution.east', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000057.current', 'scale.fictional.loan.000057', '210901.7100', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000057', 'scale.fictional.loan.000057', '754.0000', 'CNY', '2025-07-28');

INSERT INTO "party" VALUES ('scale.fictional.party.000057.primary', '虚构参与方000057主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000057.primary', 'scale.fictional.loan.000057', 'scale.fictional.party.000057.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000058', 'scale.fictional.institution.east', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000058.current', 'scale.fictional.loan.000058', '214601.7400', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000058', 'scale.fictional.loan.000058', '767.0000', 'CNY', '2025-07-31');

INSERT INTO "party" VALUES ('scale.fictional.party.000058.primary', '虚构参与方000058主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000058.primary', 'scale.fictional.loan.000058', 'scale.fictional.party.000058.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000059', 'scale.fictional.institution.east', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000059.current', 'scale.fictional.loan.000059', '218301.7700', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000059', 'scale.fictional.loan.000059', '780.0000', 'CNY', '2025-08-04');

INSERT INTO "party" VALUES ('scale.fictional.party.000059.primary', '虚构参与方000059主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000059.primary', 'scale.fictional.loan.000059', 'scale.fictional.party.000059.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000060', 'scale.fictional.institution.south', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000060.current', 'scale.fictional.loan.000060', '222001.8000', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000060', 'scale.fictional.loan.000060', '793.0000', 'CNY', '2025-08-08');

INSERT INTO "party" VALUES ('scale.fictional.party.000060.primary', '虚构参与方000060主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000060.primary', 'scale.fictional.loan.000060', 'scale.fictional.party.000060.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000061', 'scale.fictional.institution.south', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000061.current', 'scale.fictional.loan.000061', '225701.8300', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000061', 'scale.fictional.loan.000061', '806.0000', 'CNY', '2025-08-11');

INSERT INTO "party" VALUES ('scale.fictional.party.000061.primary', '虚构参与方000061主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000061.primary', 'scale.fictional.loan.000061', 'scale.fictional.party.000061.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000062', 'scale.fictional.institution.south', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000062.current', 'scale.fictional.loan.000062', '229401.8600', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000062', 'scale.fictional.loan.000062', '819.0000', 'CNY', '2025-08-15');

INSERT INTO "party" VALUES ('scale.fictional.party.000062.primary', '虚构参与方000062主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000062.primary', 'scale.fictional.loan.000062', 'scale.fictional.party.000062.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000063', 'scale.fictional.institution.south', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000063.current', 'scale.fictional.loan.000063', '233101.8900', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000063', 'scale.fictional.loan.000063', '832.0000', 'CNY', '2025-08-18');

INSERT INTO "party" VALUES ('scale.fictional.party.000063.primary', '虚构参与方000063主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000063.primary', 'scale.fictional.loan.000063', 'scale.fictional.party.000063.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000064', 'scale.fictional.institution.south', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000064.current', 'scale.fictional.loan.000064', '236801.9200', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000064', 'scale.fictional.loan.000064', '845.0000', 'CNY', '2025-08-22');

INSERT INTO "party" VALUES ('scale.fictional.party.000064.primary', '虚构参与方000064主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000064.primary', 'scale.fictional.loan.000064', 'scale.fictional.party.000064.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000065', 'scale.fictional.institution.south', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000065.current', 'scale.fictional.loan.000065', '240501.9500', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000065', 'scale.fictional.loan.000065', '858.0000', 'CNY', '2025-08-26');

INSERT INTO "party" VALUES ('scale.fictional.party.000065.primary', '虚构参与方000065主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000065.primary', 'scale.fictional.loan.000065', 'scale.fictional.party.000065.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000066', 'scale.fictional.institution.south', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000066.current', 'scale.fictional.loan.000066', '244201.9800', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000066', 'scale.fictional.loan.000066', '871.0000', 'CNY', '2025-08-29');

INSERT INTO "party" VALUES ('scale.fictional.party.000066.primary', '虚构参与方000066主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000066.primary', 'scale.fictional.loan.000066', 'scale.fictional.party.000066.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000067', 'scale.fictional.institution.south', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000067.v1', 'scale.fictional.loan.000067', '247912.0100', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000067.current', 'scale.fictional.loan.000067', '247902.0100', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000067', 'scale.fictional.loan.000067', '884.0000', 'CNY', '2025-09-02');

INSERT INTO "party" VALUES ('scale.fictional.party.000067.primary', '虚构参与方000067主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000067.primary', 'scale.fictional.loan.000067', 'scale.fictional.party.000067.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000067.co', '虚构参与方000067共', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000067.co', 'scale.fictional.loan.000067', 'scale.fictional.party.000067.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000068', 'scale.fictional.institution.south', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000068.current', 'scale.fictional.loan.000068', '251602.0400', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000068', 'scale.fictional.loan.000068', '897.0000', 'CNY', '2025-09-06');

INSERT INTO "party" VALUES ('scale.fictional.party.000068.primary', '虚构参与方000068主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000068.primary', 'scale.fictional.loan.000068', 'scale.fictional.party.000068.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000069', 'scale.fictional.institution.south', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000069.v1', 'scale.fictional.loan.000069', '255312.0700', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000069.current', 'scale.fictional.loan.000069', '255302.0700', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000069', 'scale.fictional.loan.000069', '910.0000', 'CNY', '2025-09-09');

INSERT INTO "party" VALUES ('scale.fictional.party.000069.primary', '虚构参与方000069主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000069.primary', 'scale.fictional.loan.000069', 'scale.fictional.party.000069.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000070', 'scale.fictional.institution.south', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000070.current', 'scale.fictional.loan.000070', '259002.1000', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000070', 'scale.fictional.loan.000070', '923.0000', 'CNY', '2025-09-13');

INSERT INTO "party" VALUES ('scale.fictional.party.000070.primary', '虚构参与方000070主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000070.primary', 'scale.fictional.loan.000070', 'scale.fictional.party.000070.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000071', 'scale.fictional.institution.south', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000071.v1', 'scale.fictional.loan.000071', '262712.1300', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000071.current', 'scale.fictional.loan.000071', '262702.1300', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000071', 'scale.fictional.loan.000071', '936.0000', 'CNY', '2025-09-17');

INSERT INTO "party" VALUES ('scale.fictional.party.000071.primary', '虚构参与方000071主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000071.primary', 'scale.fictional.loan.000071', 'scale.fictional.party.000071.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000072', 'scale.fictional.institution.south', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000072.current', 'scale.fictional.loan.000072', '266402.1600', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000072', 'scale.fictional.loan.000072', '949.0000', 'CNY', '2025-09-20');

INSERT INTO "party" VALUES ('scale.fictional.party.000072.primary', '虚构参与方000072主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000072.primary', 'scale.fictional.loan.000072', 'scale.fictional.party.000072.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000072.co', '虚构参与方000072共', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000072.co', 'scale.fictional.loan.000072', 'scale.fictional.party.000072.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000073', 'scale.fictional.institution.south', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000073.v1', 'scale.fictional.loan.000073', '270112.1900', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000073.current', 'scale.fictional.loan.000073', '270102.1900', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000073', 'scale.fictional.loan.000073', '962.0000', 'CNY', '2025-09-24');

INSERT INTO "party" VALUES ('scale.fictional.party.000073.primary', '虚构参与方000073主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000073.primary', 'scale.fictional.loan.000073', 'scale.fictional.party.000073.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000074', 'scale.fictional.institution.south', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000074.current', 'scale.fictional.loan.000074', '273802.2200', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000074', 'scale.fictional.loan.000074', '975.0000', 'CNY', '2025-09-28');

INSERT INTO "party" VALUES ('scale.fictional.party.000074.primary', '虚构参与方000074主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000074.primary', 'scale.fictional.loan.000074', 'scale.fictional.party.000074.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000074.co', '虚构参与方000074共', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000074.co', 'scale.fictional.loan.000074', 'scale.fictional.party.000074.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000075', 'scale.fictional.institution.south', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000075.current', 'scale.fictional.loan.000075', '277502.2500', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000075', 'scale.fictional.loan.000075', '988.0000', 'CNY', '2025-10-01');

INSERT INTO "party" VALUES ('scale.fictional.party.000075.primary', '虚构参与方000075主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000075.primary', 'scale.fictional.loan.000075', 'scale.fictional.party.000075.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000076', 'scale.fictional.institution.south', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000076.current', 'scale.fictional.loan.000076', '281202.2800', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000076', 'scale.fictional.loan.000076', '1.0000', 'CNY', '2025-10-05');

INSERT INTO "party" VALUES ('scale.fictional.party.000076.primary', '虚构参与方000076主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000076.primary', 'scale.fictional.loan.000076', 'scale.fictional.party.000076.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000076.co', '虚构参与方000076共', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000076.co', 'scale.fictional.loan.000076', 'scale.fictional.party.000076.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000077', 'scale.fictional.institution.south', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000077.current', 'scale.fictional.loan.000077', '284902.3100', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000077', 'scale.fictional.loan.000077', '14.0000', 'CNY', '2025-10-09');

INSERT INTO "party" VALUES ('scale.fictional.party.000077.primary', '虚构参与方000077主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000077.primary', 'scale.fictional.loan.000077', 'scale.fictional.party.000077.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000077.co', '虚构参与方000077共', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000077.co', 'scale.fictional.loan.000077', 'scale.fictional.party.000077.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000078', 'scale.fictional.institution.south', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000078.current', 'scale.fictional.loan.000078', '288602.3400', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000078', 'scale.fictional.loan.000078', '27.0000', 'CNY', '2025-10-12');

INSERT INTO "party" VALUES ('scale.fictional.party.000078.primary', '虚构参与方000078主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000078.primary', 'scale.fictional.loan.000078', 'scale.fictional.party.000078.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000079', 'scale.fictional.institution.south', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000079.current', 'scale.fictional.loan.000079', '292302.3700', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000079', 'scale.fictional.loan.000079', '40.0000', 'CNY', '2025-10-16');

INSERT INTO "party" VALUES ('scale.fictional.party.000079.primary', '虚构参与方000079主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000079.primary', 'scale.fictional.loan.000079', 'scale.fictional.party.000079.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000080', 'scale.fictional.institution.south', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000080.current', 'scale.fictional.loan.000080', '296002.4000', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000080', 'scale.fictional.loan.000080', '53.0000', 'CNY', '2025-10-20');

INSERT INTO "party" VALUES ('scale.fictional.party.000080.primary', '虚构参与方000080主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000080.primary', 'scale.fictional.loan.000080', 'scale.fictional.party.000080.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000081', 'scale.fictional.institution.south', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000081.current', 'scale.fictional.loan.000081', '299702.4300', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000081', 'scale.fictional.loan.000081', '66.0000', 'CNY', '2025-10-23');

INSERT INTO "party" VALUES ('scale.fictional.party.000081.primary', '虚构参与方000081主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000081.primary', 'scale.fictional.loan.000081', 'scale.fictional.party.000081.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000082', 'scale.fictional.institution.south', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000082.v1', 'scale.fictional.loan.000082', '303412.4600', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000082.current', 'scale.fictional.loan.000082', '303402.4600', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000082', 'scale.fictional.loan.000082', '79.0000', 'CNY', '2025-10-27');

INSERT INTO "party" VALUES ('scale.fictional.party.000082.primary', '虚构参与方000082主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000082.primary', 'scale.fictional.loan.000082', 'scale.fictional.party.000082.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000083', 'scale.fictional.institution.south', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000083.current', 'scale.fictional.loan.000083', '307102.4900', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000083', 'scale.fictional.loan.000083', '92.0000', 'CNY', '2025-10-30');

INSERT INTO "party" VALUES ('scale.fictional.party.000083.primary', '虚构参与方000083主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000083.primary', 'scale.fictional.loan.000083', 'scale.fictional.party.000083.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000084', 'scale.fictional.institution.south', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000084.current', 'scale.fictional.loan.000084', '310802.5200', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000084', 'scale.fictional.loan.000084', '105.0000', 'CNY', '2025-11-03');

INSERT INTO "party" VALUES ('scale.fictional.party.000084.primary', '虚构参与方000084主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000084.primary', 'scale.fictional.loan.000084', 'scale.fictional.party.000084.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000085', 'scale.fictional.institution.south', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000085.current', 'scale.fictional.loan.000085', '314502.5500', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000085', 'scale.fictional.loan.000085', '118.0000', 'CNY', '2025-11-07');

INSERT INTO "party" VALUES ('scale.fictional.party.000085.primary', '虚构参与方000085主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000085.primary', 'scale.fictional.loan.000085', 'scale.fictional.party.000085.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000086', 'scale.fictional.institution.south', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000086.current', 'scale.fictional.loan.000086', '318202.5800', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000086', 'scale.fictional.loan.000086', '131.0000', 'CNY', '2025-11-10');

INSERT INTO "party" VALUES ('scale.fictional.party.000086.primary', '虚构参与方000086主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000086.primary', 'scale.fictional.loan.000086', 'scale.fictional.party.000086.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000087', 'scale.fictional.institution.south', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000087.current', 'scale.fictional.loan.000087', '321902.6100', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000087', 'scale.fictional.loan.000087', '144.0000', 'CNY', '2025-11-14');

INSERT INTO "party" VALUES ('scale.fictional.party.000087.primary', '虚构参与方000087主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000087.primary', 'scale.fictional.loan.000087', 'scale.fictional.party.000087.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000088', 'scale.fictional.institution.south', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000088.current', 'scale.fictional.loan.000088', '325602.6400', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000088', 'scale.fictional.loan.000088', '157.0000', 'CNY', '2025-11-18');

INSERT INTO "party" VALUES ('scale.fictional.party.000088.primary', '虚构参与方000088主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000088.primary', 'scale.fictional.loan.000088', 'scale.fictional.party.000088.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000089', 'scale.fictional.institution.south', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000089.current', 'scale.fictional.loan.000089', '329302.6700', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000089', 'scale.fictional.loan.000089', '170.0000', 'CNY', '2025-11-21');

INSERT INTO "party" VALUES ('scale.fictional.party.000089.primary', '虚构参与方000089主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000089.primary', 'scale.fictional.loan.000089', 'scale.fictional.party.000089.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000090', 'scale.fictional.institution.south', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000090.v1', 'scale.fictional.loan.000090', '333012.7000', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000090.current', 'scale.fictional.loan.000090', '333002.7000', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000090', 'scale.fictional.loan.000090', '183.0000', 'CNY', '2025-11-25');

INSERT INTO "party" VALUES ('scale.fictional.party.000090.primary', '虚构参与方000090主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000090.primary', 'scale.fictional.loan.000090', 'scale.fictional.party.000090.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000091', 'scale.fictional.institution.south', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000091.current', 'scale.fictional.loan.000091', '336702.7300', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000091', 'scale.fictional.loan.000091', '196.0000', 'CNY', '2025-11-29');

INSERT INTO "party" VALUES ('scale.fictional.party.000091.primary', '虚构参与方000091主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000091.primary', 'scale.fictional.loan.000091', 'scale.fictional.party.000091.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000092', 'scale.fictional.institution.south', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000092.v1', 'scale.fictional.loan.000092', '340412.7600', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000092.current', 'scale.fictional.loan.000092', '340402.7600', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000092', 'scale.fictional.loan.000092', '209.0000', 'CNY', '2025-12-02');

INSERT INTO "party" VALUES ('scale.fictional.party.000092.primary', '虚构参与方000092主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000092.primary', 'scale.fictional.loan.000092', 'scale.fictional.party.000092.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000093', 'scale.fictional.institution.south', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000093.current', 'scale.fictional.loan.000093', '344102.7900', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000093', 'scale.fictional.loan.000093', '222.0000', 'CNY', '2025-12-06');

INSERT INTO "party" VALUES ('scale.fictional.party.000093.primary', '虚构参与方000093主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000093.primary', 'scale.fictional.loan.000093', 'scale.fictional.party.000093.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000094', 'scale.fictional.institution.south', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000094.current', 'scale.fictional.loan.000094', '347802.8200', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000094', 'scale.fictional.loan.000094', '235.0000', 'CNY', '2025-12-10');

INSERT INTO "party" VALUES ('scale.fictional.party.000094.primary', '虚构参与方000094主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000094.primary', 'scale.fictional.loan.000094', 'scale.fictional.party.000094.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000094.co', '虚构参与方000094共', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000094.co', 'scale.fictional.loan.000094', 'scale.fictional.party.000094.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000095', 'scale.fictional.institution.south', 'scale.fictional.lender.000');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000095.current', 'scale.fictional.loan.000095', '351502.8500', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000095', 'scale.fictional.loan.000095', '248.0000', 'CNY', '2025-12-13');

INSERT INTO "party" VALUES ('scale.fictional.party.000095.primary', '虚构参与方000095主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000095.primary', 'scale.fictional.loan.000095', 'scale.fictional.party.000095.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000096', 'scale.fictional.institution.south', 'scale.fictional.lender.001');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000096.current', 'scale.fictional.loan.000096', '355202.8800', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000096', 'scale.fictional.loan.000096', '261.0000', 'CNY', '2025-12-17');

INSERT INTO "party" VALUES ('scale.fictional.party.000096.primary', '虚构参与方000096主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000096.primary', 'scale.fictional.loan.000096', 'scale.fictional.party.000096.primary', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "party" VALUES ('scale.fictional.party.000096.co', '虚构参与方000096共', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000096.co', 'scale.fictional.loan.000096', 'scale.fictional.party.000096.co', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000097', 'scale.fictional.institution.south', 'scale.fictional.lender.002');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000097.v1', 'scale.fictional.loan.000097', '358912.9100', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000097.current', 'scale.fictional.loan.000097', '358902.9100', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000097', 'scale.fictional.loan.000097', '274.0000', 'CNY', '2025-12-21');

INSERT INTO "party" VALUES ('scale.fictional.party.000097.primary', '虚构参与方000097主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000097.primary', 'scale.fictional.loan.000097', 'scale.fictional.party.000097.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000098', 'scale.fictional.institution.south', 'scale.fictional.lender.003');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000098.v1', 'scale.fictional.loan.000098', '362612.9400', 'CNY', '2025-01-01', '2025-06-15');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000098.current', 'scale.fictional.loan.000098', '362602.9400', 'CNY', '2025-06-15', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000098', 'scale.fictional.loan.000098', '287.0000', 'CNY', '2025-12-24');

INSERT INTO "party" VALUES ('scale.fictional.party.000098.primary', '虚构参与方000098主', 'scale.fictional.region.south');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000098.primary', 'scale.fictional.loan.000098', 'scale.fictional.party.000098.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_contract" VALUES ('scale.fictional.loan.000099', 'scale.fictional.institution.south', 'scale.fictional.lender.004');

INSERT INTO "balance_observation" VALUES ('scale.fictional.balance.000099.current', 'scale.fictional.loan.000099', '366302.9700', 'CNY', '2025-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('scale.fictional.disbursement.000099', 'scale.fictional.loan.000099', '300.0000', 'CNY', '2025-12-28');

INSERT INTO "party" VALUES ('scale.fictional.party.000099.primary', '虚构参与方000099主', 'scale.fictional.region.east');

INSERT INTO "loan_participation" VALUES ('scale.fictional.participation.000099.primary', 'scale.fictional.loan.000099', 'scale.fictional.party.000099.primary', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "identity_dataset_version" VALUES ('scale.fictional.identity-v1', NULL, '2025-01-01', 'True');

INSERT INTO "source_lender_description" VALUES ('scale.fictional.source.core.000', 'core', 'FICTIONAL-000', '虚构贷款人000', 'FICTIONAL-TAX-000');

INSERT INTO "identity_decision" VALUES ('scale.fictional.identity-v1', 'scale.fictional.source.core.000', 'scale.fictional.lender.000', 'synthetic-scale-link', 'accepted');

INSERT INTO "source_lender_description" VALUES ('scale.fictional.source.crm.000', 'crm', 'FICTIONAL-000', '虚构贷款人000', 'FICTIONAL-TAX-000');

INSERT INTO "identity_decision" VALUES ('scale.fictional.identity-v1', 'scale.fictional.source.crm.000', 'scale.fictional.lender.000', 'synthetic-scale-link', 'accepted');

INSERT INTO "source_lender_description" VALUES ('scale.fictional.source.core.001', 'core', 'FICTIONAL-001', '虚构贷款人001', 'FICTIONAL-TAX-001');

INSERT INTO "identity_decision" VALUES ('scale.fictional.identity-v1', 'scale.fictional.source.core.001', 'scale.fictional.lender.001', 'synthetic-scale-link', 'accepted');

INSERT INTO "source_lender_description" VALUES ('scale.fictional.source.crm.001', 'crm', 'FICTIONAL-001', '虚构贷款人001', 'FICTIONAL-TAX-001');

INSERT INTO "identity_decision" VALUES ('scale.fictional.identity-v1', 'scale.fictional.source.crm.001', 'scale.fictional.lender.001', 'synthetic-scale-link', 'accepted');

INSERT INTO "source_lender_description" VALUES ('scale.fictional.source.core.002', 'core', 'FICTIONAL-002', '虚构贷款人002', 'FICTIONAL-TAX-002');

INSERT INTO "identity_decision" VALUES ('scale.fictional.identity-v1', 'scale.fictional.source.core.002', 'scale.fictional.lender.002', 'synthetic-scale-link', 'accepted');

INSERT INTO "source_lender_description" VALUES ('scale.fictional.source.crm.002', 'crm', 'FICTIONAL-002', '虚构贷款人002', 'FICTIONAL-TAX-002');

INSERT INTO "identity_decision" VALUES ('scale.fictional.identity-v1', 'scale.fictional.source.crm.002', 'scale.fictional.lender.002', 'synthetic-scale-link', 'accepted');

INSERT INTO "source_lender_description" VALUES ('scale.fictional.source.core.003', 'core', 'FICTIONAL-003', '虚构贷款人003', 'FICTIONAL-TAX-003');

INSERT INTO "identity_decision" VALUES ('scale.fictional.identity-v1', 'scale.fictional.source.core.003', 'scale.fictional.lender.003', 'synthetic-scale-link', 'accepted');

INSERT INTO "source_lender_description" VALUES ('scale.fictional.source.crm.003', 'crm', 'FICTIONAL-003', '虚构贷款人003', 'FICTIONAL-TAX-003');

INSERT INTO "identity_decision" VALUES ('scale.fictional.identity-v1', 'scale.fictional.source.crm.003', 'scale.fictional.lender.003', 'synthetic-scale-link', 'accepted');

INSERT INTO "source_lender_description" VALUES ('scale.fictional.source.core.004', 'core', 'FICTIONAL-004', '虚构贷款人004', 'FICTIONAL-TAX-004');

INSERT INTO "identity_decision" VALUES ('scale.fictional.identity-v1', 'scale.fictional.source.core.004', 'scale.fictional.lender.004', 'synthetic-scale-link', 'accepted');

INSERT INTO "source_lender_description" VALUES ('scale.fictional.source.crm.004', 'crm', 'FICTIONAL-004', '虚构贷款人004', 'FICTIONAL-TAX-004');

INSERT INTO "identity_decision" VALUES ('scale.fictional.identity-v1', 'scale.fictional.source.crm.004', 'scale.fictional.lender.004', 'synthetic-scale-link', 'accepted');

CREATE FUNCTION reject_identity_history_mutation() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'identity history is immutable';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER "identity_dataset_version_immutable" BEFORE UPDATE OR DELETE ON "identity_dataset_version" FOR EACH ROW EXECUTE FUNCTION reject_identity_history_mutation();

CREATE TRIGGER "identity_decision_immutable" BEFORE UPDATE OR DELETE ON "identity_decision" FOR EACH ROW EXECUTE FUNCTION reject_identity_history_mutation();

CREATE FUNCTION "reject_identity_decision_sealed_insert"() RETURNS trigger AS $$
BEGIN
  IF EXISTS (SELECT 1 FROM "identity_dataset_version"
             WHERE "dataset_version" = NEW."dataset_version"
               AND "sealed") THEN
    RAISE EXCEPTION 'identity dataset version is sealed';
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER "identity_decision_sealed_insert" BEFORE INSERT ON "identity_decision" FOR EACH ROW EXECUTE FUNCTION "reject_identity_decision_sealed_insert"();

UPDATE scale_load_audit SET finished_at = clock_timestamp();
