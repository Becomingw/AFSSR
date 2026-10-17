-- Additive only: old aggregate admissions retain NULL model and stay readable.
ALTER TABLE public.usage_request_admissions
    ADD COLUMN IF NOT EXISTS model character varying(128);
CREATE INDEX IF NOT EXISTS usage_request_admissions_subject_model_time_idx
    ON public.usage_request_admissions (subject_id, model, admitted_at);
