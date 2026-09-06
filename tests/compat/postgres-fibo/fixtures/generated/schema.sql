\connect fibo

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

INSERT INTO "region" VALUES ('region.east', '华东');

INSERT INTO "region" VALUES ('region.south', '华南');

INSERT INTO "booking_institution" VALUES ('institution.east', '华东入账机构', 'region.east');

INSERT INTO "lender" VALUES ('lender.alpha', '同名贷款人');

INSERT INTO "lender" VALUES ('lender.beta', '同名贷款人');

INSERT INTO "loan_contract" VALUES ('loan.L1', 'institution.east', 'lender.alpha');

INSERT INTO "loan_contract" VALUES ('loan.L2', 'institution.east', 'lender.beta');

INSERT INTO "balance_observation" VALUES ('balance.L1.2025-01-01', 'loan.L1', '100.00', 'CNY', '2025-01-01', '2025-07-01');

INSERT INTO "balance_observation" VALUES ('balance.L1.2025-07-01', 'loan.L1', '80.00', 'CNY', '2025-07-01', '2030-01-01');

INSERT INTO "balance_observation" VALUES ('balance.L2.2025-01-01', 'loan.L2', '50.00', 'CNY', '2025-01-01', '2030-01-01');

INSERT INTO "balance_observation" VALUES ('balance.L1.2030-01-01', 'loan.L1', '100.00', 'CNY', '2030-01-01', NULL);

INSERT INTO "balance_observation" VALUES ('balance.L2.2030-01-01', 'loan.L2', '100.00', 'CNY', '2030-01-01', NULL);

INSERT INTO "disbursement_event" VALUES ('disbursement.2024', 'loan.L1', '900.00', 'CNY', '2024-12-31');

INSERT INTO "disbursement_event" VALUES ('disbursement.L1', 'loan.L1', '70.00', 'CNY', '2025-01-01');

INSERT INTO "disbursement_event" VALUES ('disbursement.L2', 'loan.L2', '30.00', 'CNY', '2025-12-31');

INSERT INTO "disbursement_event" VALUES ('disbursement.2026', 'loan.L1', '999.00', 'CNY', '2026-01-01');

INSERT INTO "disbursement_event" VALUES ('rounding.equal.1', 'loan.L1', '0.0051', 'CNY', '2040-01-01');

INSERT INTO "disbursement_event" VALUES ('rounding.equal.2', 'loan.L2', '0.0051', 'CNY', '2040-01-02');

INSERT INTO "disbursement_event" VALUES ('rounding.zero', 'loan.L1', '0.0000', 'CNY', '2040-01-03');

INSERT INTO "region_alias" VALUES ('region.east', '东部');

INSERT INTO "region_alias" VALUES ('region.east', '华东地区');

INSERT INTO "source_lender_description" VALUES ('source.crm.alpha', 'crm', 'PARTY-100', '同名贷款人', 'TAX-001');

INSERT INTO "source_lender_description" VALUES ('source.core.alpha', 'core', 'PARTY-100A', '同名贷款人', 'TAX-001');

INSERT INTO "source_lender_description" VALUES ('source.legacy.beta', 'legacy', 'PARTY-100', '同名贷款人', 'TAX-002');

INSERT INTO "identity_dataset_version" VALUES ('identity-v1', NULL, '2030-01-01', 'True');

INSERT INTO "identity_dataset_version" VALUES ('identity-v2', 'identity-v1', '2031-01-01', 'True');

INSERT INTO "identity_decision" VALUES ('identity-v1', 'source.crm.alpha', 'lender.alpha', 'verified-registry:TAX-001', 'accepted');

INSERT INTO "identity_decision" VALUES ('identity-v1', 'source.core.alpha', 'lender.alpha', 'verified-registry:TAX-001', 'accepted');

INSERT INTO "identity_decision" VALUES ('identity-v1', 'source.legacy.beta', 'lender.beta', 'verified-registry:TAX-002', 'accepted');

INSERT INTO "identity_decision" VALUES ('identity-v2', 'source.crm.alpha', 'lender.alpha', 'merge-approval:MERGE-2031-01', 'accepted');

INSERT INTO "identity_decision" VALUES ('identity-v2', 'source.core.alpha', 'lender.alpha', 'merge-approval:MERGE-2031-01', 'accepted');

INSERT INTO "identity_decision" VALUES ('identity-v2', 'source.legacy.beta', 'lender.alpha', 'merge-approval:MERGE-2031-01', 'accepted');

INSERT INTO "party" VALUES ('party.P1', '虚构主借款人甲', 'region.east');

INSERT INTO "party" VALUES ('party.P2', '虚构共同借款人乙', 'region.south');

INSERT INTO "party" VALUES ('party.P3', '虚构主借款人丙', 'region.east');

INSERT INTO "loan_participation" VALUES ('participation.L1.primary', 'loan.L1', 'party.P1', 'primary-borrower', '2025-01-01', NULL, '0.600000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_participation" VALUES ('participation.L1.co', 'loan.L1', 'party.P2', 'co-borrower', '2025-01-01', NULL, '0.400000', 'loan-balance-approved-v1', '0.500000');

INSERT INTO "loan_participation" VALUES ('participation.L2.primary', 'loan.L2', 'party.P3', 'primary-borrower', '2025-01-01', NULL, '1.000000', 'loan-balance-approved-v1', '1.000000');

INSERT INTO "loan_participation" VALUES ('participation.L2.guarantor', 'loan.L2', 'party.P1', 'guarantor', '2025-01-01', NULL, NULL, NULL, NULL);

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
