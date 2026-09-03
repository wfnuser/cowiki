import { useState, useEffect } from 'react';
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from '@/components/ui/select';
import { AvatarBadge } from '@/components/ui/avatar-badge';
import { listMembers, initiateTransfer, type MemberInfo } from '../api';
import { C } from '@/lib/design';

interface TransferDialogProps {
  open: boolean;
  workspaceSlug: string;
  workspaceName: string;
  currentUserId: string;
  onOpenChange: (open: boolean) => void;
  onSuccess: (msg: string) => void;
  onError: (msg: string) => void;
}

export function TransferDialog({
  open,
  workspaceSlug,
  workspaceName,
  currentUserId,
  onOpenChange,
  onSuccess,
  onError,
}: TransferDialogProps) {
  const [members, setMembers] = useState<MemberInfo[]>([]);
  const [selectedId, setSelectedId] = useState('');
  const [newRole, setNewRole] = useState('manager');
  const [sending, setSending] = useState(false);

  useEffect(() => {
    if (open) {
      setSelectedId('');
      setNewRole('manager');
      setSending(false);
      listMembers(workspaceSlug)
        .then((m) => setMembers(m.filter((u) => u.id !== currentUserId)))
        .catch(() => setMembers([]));
    }
  }, [open, workspaceSlug, currentUserId]);

  const handleTransfer = async () => {
    if (!selectedId || sending) return;
    setSending(true);
    try {
      await initiateTransfer(workspaceSlug, selectedId, newRole);
      onSuccess('Ownership transfer initiated.');
      onOpenChange(false);
    } catch (error: unknown) {
      onError(error instanceof Error ? error.message : 'Failed to initiate transfer');
    } finally {
      setSending(false);
    }
  };

  const selectedMember = members.find((m) => m.id === selectedId);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Transfer Ownership — {workspaceName}</DialogTitle>
          <DialogDescription>
            Transfer ownership to another member. You will be assigned a new role.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 mt-2">
          {/* Member selection */}
          <div>
            <label className="mb-1.5 block text-sm text-text-secondary">
              New Owner
            </label>
            <Select value={selectedId} onValueChange={setSelectedId}>
              <SelectTrigger>
                <SelectValue placeholder="Select a member..." />
              </SelectTrigger>
              <SelectContent>
                {members.map((m) => (
                  <SelectItem key={m.id} value={m.id}>
                    <span style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                      <AvatarBadge name={m.name} size={22} kind="member" />
                      <span>{m.name}</span>
                      <span style={{ color: C.muted, fontSize: 11 }}>({m.role})</span>
                    </span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {/* Selected member info */}
          {selectedMember && (
            <div style={{
              display: 'flex', alignItems: 'center', gap: 10,
              padding: '10px 12px', borderRadius: 8,
              background: C.sidebar, border: `1px solid ${C.line}`,
            }}>
              <AvatarBadge name={selectedMember.name} size={32} kind="member" />
              <div style={{ minWidth: 0 }}>
                <div style={{ fontSize: 14, fontWeight: 550, color: C.ink }}>
                  {selectedMember.name}
                </div>
                <div style={{ fontSize: 12, color: C.muted }}>
                  {selectedMember.email || 'No email'}
                </div>
              </div>
            </div>
          )}

          {/* Your new role */}
          <div>
            <label className="mb-1.5 block text-sm text-text-secondary">
              Your new role after transfer
            </label>
            <Select value={newRole} onValueChange={setNewRole}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="manager">Manager</SelectItem>
                <SelectItem value="editor">Editor</SelectItem>
                <SelectItem value="viewer">Viewer</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div style={{
            padding: '8px 12px', borderRadius: 6,
            background: C.amberSoft, color: C.amber, fontSize: 12,
          }}>
            This action cannot be undone. The new owner must accept the transfer.
          </div>

          <div className="flex justify-end gap-2 pt-2">
            <Button variant="outline" onClick={() => onOpenChange(false)}>Cancel</Button>
            <Button onClick={handleTransfer} disabled={!selectedId || sending}>
              {sending ? 'Initiating...' : 'Transfer Ownership'}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

export default TransferDialog;
