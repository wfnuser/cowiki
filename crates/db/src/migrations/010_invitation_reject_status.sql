-- Allow invitations to be rejected. `reject_invitation` writes status = 'rejected',
-- but the original CHECK (002_workspaces.sql) only permitted ('pending','accepted','expired'),
-- so the reject-invitation endpoint failed at runtime with a check_violation.
ALTER TABLE invitations DROP CONSTRAINT IF EXISTS invitations_status_check;
ALTER TABLE invitations ADD CONSTRAINT invitations_status_check
    CHECK (status IN ('pending', 'accepted', 'expired', 'rejected'));
