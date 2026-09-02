import { useState, useRef, useEffect, useCallback } from 'react';
import { Search, X, Check } from 'lucide-react';
import {
  Dialog, DialogContent, DialogHeader, DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from '@/components/ui/select';
import { AvatarBadge } from '@/components/ui/avatar-badge';
import { searchUsers, inviteToWorkspace, type UserSearchResult } from '../api';
import { C, shadows } from '@/lib/design';

// ── Props ──

interface InviteDialogProps {
  open: boolean;
  workspaceName: string;
  workspaceSlug: string;
  onOpenChange: (open: boolean) => void;
  onInvited: (message: string) => void;
  onError: (message: string) => void;
}

// ── Component ──

export function InviteDialog({
  open,
  workspaceName,
  workspaceSlug,
  onOpenChange,
  onInvited,
  onError,
}: InviteDialogProps) {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<UserSearchResult[]>([]);
  const [selected, setSelected] = useState<UserSearchResult | null>(null);
  const [role, setRole] = useState('viewer');
  const [expires, setExpires] = useState('7');
  const [searching, setSearching] = useState(false);
  const [sending, setSending] = useState(false);
  const [focusedIdx, setFocusedIdx] = useState(-1);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  // Search on input change
  const doSearch = useCallback(async (q: string) => {
    if (q.length < 1) {
      setResults([]);
      setFocusedIdx(-1);
      return;
    }
    setSearching(true);
    try {
      const users = await searchUsers(q, 8);
      setResults(users.filter((u) => u.id !== selected?.id));
      setFocusedIdx(-1);
    } catch {
      setResults([]);
    } finally {
      setSearching(false);
    }
  }, [selected?.id]);

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => doSearch(query), 200);
    return () => { if (debounceRef.current) clearTimeout(debounceRef.current); };
  }, [query, doSearch]);

  // Reset on open
  useEffect(() => {
    if (open) {
      setQuery('');
      setSelected(null);
      setResults([]);
      setRole('viewer');
      setExpires('7');
      setFocusedIdx(-1);
    }
  }, [open]);

  // Keyboard nav
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setFocusedIdx((prev) => Math.min(prev + 1, results.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setFocusedIdx((prev) => Math.max(prev - 1, -1));
    } else if (e.key === 'Enter' && focusedIdx >= 0 && focusedIdx < results.length) {
      e.preventDefault();
      selectUser(results[focusedIdx]);
    } else if (e.key === 'Escape') {
      setResults([]);
      setFocusedIdx(-1);
    }
  };

  const selectUser = (user: UserSearchResult) => {
    setSelected(user);
    setQuery('');
    setResults([]);
    setFocusedIdx(-1);
    inputRef.current?.focus();
  };

  const clearSelection = () => {
    setSelected(null);
    inputRef.current?.focus();
  };

  const handleSend = async () => {
    if (!selected || sending) return;
    setSending(true);
    try {
      await inviteToWorkspace(workspaceSlug, selected.id, role, Number(expires));
      onInvited(`Invitation sent to ${selected.name}.`);
      onOpenChange(false);
    } catch (error: unknown) {
      onError(error instanceof Error ? error.message : 'Failed to send invitation');
    } finally {
      setSending(false);
    }
  };

  // Scroll focused item into view
  useEffect(() => {
    if (focusedIdx >= 0 && listRef.current) {
      const items = listRef.current.children;
      if (items[focusedIdx]) {
        (items[focusedIdx] as HTMLElement).scrollIntoView({ block: 'nearest' });
      }
    }
  }, [focusedIdx]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Invite Member — {workspaceName}</DialogTitle>
        </DialogHeader>

        <div className="space-y-4 mt-2">
          {/* User search input */}
          <div>
            <label className="mb-1.5 block text-sm text-text-secondary">
              User
            </label>
            {selected ? (
              // Selected user chip
              <div
                style={{
                  display: 'flex', alignItems: 'center', gap: 10,
                  padding: '8px 12px', borderRadius: 8,
                  border: `1px solid ${C.line}`, background: C.sidebar,
                }}
              >
                <AvatarBadge name={selected.name} size={32} kind="member" />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: 14, fontWeight: 550, color: C.ink }}>
                    {selected.name}
                  </div>
                  <div style={{ fontSize: 12, color: C.muted, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {selected.email || 'No email'}
                  </div>
                </div>
                <button
                  onClick={clearSelection}
                  style={{
                    background: 'none', border: 'none', cursor: 'pointer',
                    color: C.muted, padding: 2, borderRadius: 4,
                  }}
                >
                  <X size={16} />
                </button>
              </div>
            ) : (
              // Search input
              <div style={{ position: 'relative' }}>
                <div style={{
                  display: 'flex', alignItems: 'center', gap: 8,
                  padding: '8px 12px', borderRadius: 8,
                  border: `1px solid ${C.line}`, background: C.panel,
                }}>
                  <Search size={15} color={C.faint} />
                  <input
                    ref={inputRef}
                    type="text"
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    onKeyDown={handleKeyDown}
                    placeholder="Search by name, email, or ID..."
                    autoFocus
                    style={{
                      flex: 1, border: 'none', outline: 'none',
                      fontSize: 14, color: C.ink, background: 'transparent',
                    }}
                  />
                  {searching && (
                    <div style={{
                      width: 16, height: 16, border: `2px solid ${C.line}`,
                      borderTopColor: C.accent, borderRadius: '50%',
                      animation: 'spin 0.6s linear infinite',
                    }} />
                  )}
                </div>

                {/* Results dropdown */}
                {results.length > 0 && (
                  <div
                    ref={listRef}
                    style={{
                      position: 'absolute', top: '100%', left: 0, right: 0,
                      marginTop: 4, zIndex: 300,
                      border: `1px solid ${C.line}`, borderRadius: 8,
                      background: C.panel, boxShadow: shadows.lifted,
                      maxHeight: 240, overflow: 'auto',
                    }}
                  >
                    {results.map((user, i) => (
                      <button
                        key={user.id}
                        onClick={() => selectUser(user)}
                        onMouseEnter={() => setFocusedIdx(i)}
                        style={{
                          display: 'flex', alignItems: 'center', gap: 10,
                          width: '100%', padding: '10px 12px',
                          border: 'none', cursor: 'pointer',
                          textAlign: 'left',
                          background: i === focusedIdx ? C.rail : 'transparent',
                          borderBottom: i < results.length - 1 ? `1px solid ${C.lineSoft}` : 'none',
                        }}
                      >
                        <AvatarBadge name={user.name} size={30} kind="member" />
                        <div style={{ minWidth: 0 }}>
                          <div style={{ fontSize: 14, fontWeight: 550, color: C.ink }}>
                            {user.name}
                          </div>
                          <div style={{
                            fontSize: 12, color: C.muted,
                            overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                          }}>
                            {user.email || 'No email'} · {user.id.slice(0, 8)}...
                          </div>
                        </div>
                        {i === focusedIdx && (
                          <Check size={14} color={C.accent} style={{ marginLeft: 'auto', flexShrink: 0 }} />
                        )}
                      </button>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>

          {/* Role + Expires */}
          {selected && (
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
              <div>
                <label className="mb-1.5 block text-sm text-text-secondary">Role</label>
                <Select value={role} onValueChange={setRole}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="viewer">Viewer</SelectItem>
                    <SelectItem value="editor">Editor</SelectItem>
                    <SelectItem value="manager">Manager</SelectItem>
                    <SelectItem value="owner">Owner</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div>
                <label className="mb-1.5 block text-sm text-text-secondary">Expires</label>
                <Select value={expires} onValueChange={setExpires}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="1">1 day</SelectItem>
                    <SelectItem value="3">3 days</SelectItem>
                    <SelectItem value="7">7 days</SelectItem>
                    <SelectItem value="30">30 days</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>
          )}

          {/* Actions */}
          <div className="flex justify-end gap-2 pt-2">
            <Button variant="outline" type="button" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button
              type="button"
              onClick={handleSend}
              disabled={!selected || sending || role === 'owner'}
            >
              {sending ? 'Sending...' : 'Send Invitation'}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

export default InviteDialog;
