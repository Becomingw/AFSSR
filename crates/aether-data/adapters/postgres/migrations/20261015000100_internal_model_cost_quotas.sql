ALTER TABLE public.usage_cost_reservations
    ADD COLUMN IF NOT EXISTS model VARCHAR(128);

CREATE INDEX IF NOT EXISTS usage_cost_reservations_subject_model_admitted_at_idx
    ON public.usage_cost_reservations (subject_id, model, admitted_at);
