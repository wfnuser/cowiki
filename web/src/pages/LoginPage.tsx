import { useEffect } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { getStoredAuth, storeAuth } from '../auth';
import {
  AUTH_RETURN_PATH_STORAGE,
  buildWebGithubLoginUrl,
  safeAuthReturnPath,
} from '../auth-flow';
import { startDesktopGithubOAuth } from '../desktop-auth';
import { apiBase, isDesktopClient } from '../runtime';
import { C } from '@/lib/design';

export function LoginPage() {
  const navigate = useNavigate();
  const location = useLocation();
  const desktop = isDesktopClient();
  const returnPath = safeAuthReturnPath(new URLSearchParams(location.search).get('returnTo'));

  useEffect(() => {
    if (getStoredAuth()) {
      navigate(desktop ? '/' : returnPath, { replace: true });
    }
  }, [desktop, navigate, returnPath]);

  const signInWithGitHub = async (event: React.MouseEvent<HTMLAnchorElement>) => {
    if (!desktop) {
      window.sessionStorage.setItem(AUTH_RETURN_PATH_STORAGE, returnPath);
      return;
    }
    event.preventDefault();
    const credential = await startDesktopGithubOAuth();
    storeAuth(credential.apiKey, credential.userName, credential.userId);
    navigate('/', { replace: true });
  };

  return (
    <main
      data-tauri-drag-region="deep"
      style={{ minHeight: '100vh', display: 'grid', placeItems: 'center', padding: 32, background: C.rail }}
    >
      <section style={shellStyle}>
        <div style={brandPanelStyle}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 11 }}>
            <img src="/cowiki-logo.svg" alt="CoWiki" width={34} height={34} />
            <span style={{ color: C.ink, fontFamily: 'var(--font-serif)', fontSize: 25, fontWeight: 700 }}>
              CoWiki
            </span>
          </div>
          <div>
            <p style={{ margin: '0 0 12px', color: C.accent, fontSize: 12, fontWeight: 700, letterSpacing: '0.08em' }}>
              LOCAL FIRST
            </p>
            <h1 style={{ maxWidth: 380, margin: 0, color: C.ink, fontFamily: 'var(--font-serif)', fontSize: 38, lineHeight: 1.12, letterSpacing: '-0.025em' }}>
              Your knowledge stays yours.
            </h1>
            <p style={{ maxWidth: 390, margin: '18px 0 0', color: C.muted, fontSize: 14, lineHeight: 1.7 }}>
              Work in local Spaces without an account. Connect only when you want to publish or collaborate.
            </p>
          </div>
          <p style={{ margin: 0, color: C.faint, fontSize: 11.5 }}>人与 AI 共建的知识空间</p>
        </div>

        <div style={formPanelStyle}>
          <div style={{ width: 'min(340px, 100%)' }}>
            <div style={{ width: 42, height: 42, display: 'grid', placeItems: 'center', marginBottom: 22, borderRadius: 12, background: C.accentSoft, color: C.accent }}>
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
                <path d="M7 18a4 4 0 0 1 0-8 5 5 0 0 1 9.6-1.3A3.5 3.5 0 0 1 17.5 18Z" />
              </svg>
            </div>
            <h2 style={{ margin: 0, color: C.ink, fontFamily: 'var(--font-serif)', fontSize: 30, lineHeight: 1.2 }}>
              Sign in
            </h2>
            <p style={{ margin: '9px 0 26px', color: C.muted, fontSize: 13.5, lineHeight: 1.6 }}>
              Connect to CoWiki Cloud to publish or join a shared Space.
            </p>

            <a
              href={buildWebGithubLoginUrl(apiBase())}
              onClick={signInWithGitHub}
              style={githubButtonStyle}
            >
              <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
                <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
              </svg>
              {desktop ? 'Continue with GitHub' : 'Sign in with GitHub'}
            </a>

            {desktop && (
              <button type="button" onClick={() => navigate('/', { replace: true })} style={localButtonStyle}>
                Continue locally
              </button>
            )}

            <p style={{ margin: '22px 0 0', color: C.faint, fontSize: 11.5, lineHeight: 1.55 }}>
              Signing in does not upload a local Space. Publishing remains an explicit action.
            </p>
          </div>
        </div>
      </section>
    </main>
  );
}

const shellStyle: React.CSSProperties = {
  width: 'min(980px, 100%)',
  minHeight: 590,
  display: 'grid',
  gridTemplateColumns: '1.05fr 0.95fr',
  overflow: 'hidden',
  border: `1px solid ${C.line}`,
  borderRadius: 16,
  background: C.panel,
  boxShadow: '0 24px 70px rgba(29, 28, 26, 0.16), 0 2px 8px rgba(29, 28, 26, 0.08)',
};

const brandPanelStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  justifyContent: 'space-between',
  padding: '52px 54px 42px',
  borderRight: `1px solid ${C.line}`,
  background: C.sidebar,
};

const formPanelStyle: React.CSSProperties = {
  display: 'grid',
  placeItems: 'center',
  padding: 48,
  background: C.panel,
};

const githubButtonStyle: React.CSSProperties = {
  width: '100%',
  height: 44,
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
  gap: 10,
  borderRadius: 9,
  background: C.ink,
  color: '#fff',
  fontSize: 13.5,
  fontWeight: 650,
  textDecoration: 'none',
};

const localButtonStyle: React.CSSProperties = {
  width: '100%',
  height: 42,
  marginTop: 10,
  border: `1px solid ${C.line}`,
  borderRadius: 9,
  background: C.panel,
  color: C.ink2,
  fontSize: 13,
  fontWeight: 600,
  cursor: 'pointer',
};
