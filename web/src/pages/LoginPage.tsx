import { useEffect, useRef, useState } from 'react';
import { LoaderCircle } from 'lucide-react';
import { useLocation, useNavigate } from 'react-router-dom';
import { getStoredAuth, storeAuth } from '../auth';
import {
  AUTH_RETURN_PATH_STORAGE,
  buildWebGithubLoginUrl,
  describeDesktopOAuthError,
  safeAuthReturnPath,
} from '../auth-flow';
import { startDesktopGithubOAuth } from '../desktop-auth';
import { apiBase, isDesktopClient } from '../runtime';
import './LoginPage.css';

export function LoginPage() {
  const navigate = useNavigate();
  const location = useLocation();
  const desktop = isDesktopClient();
  const returnPath = safeAuthReturnPath(new URLSearchParams(location.search).get('returnTo'));
  const [signingIn, setSigningIn] = useState(false);
  const [error, setError] = useState('');
  const attemptRef = useRef(0);

  useEffect(() => {
    if (getStoredAuth()) {
      navigate(desktop ? '/' : returnPath, { replace: true });
    }
  }, [desktop, navigate, returnPath]);

  useEffect(() => () => {
    // Ignore late OAuth resolutions after leaving the sign-in screen.
    attemptRef.current += 1;
  }, []);

  const signInWithGitHub = async (event: React.MouseEvent<HTMLAnchorElement>) => {
    if (!desktop) {
      window.sessionStorage.setItem(AUTH_RETURN_PATH_STORAGE, returnPath);
      return;
    }
    event.preventDefault();
    if (signingIn) return;
    const attempt = ++attemptRef.current;
    setSigningIn(true);
    setError('');
    try {
      const credential = await startDesktopGithubOAuth();
      if (attemptRef.current !== attempt) return;
      storeAuth(credential.apiKey, credential.userName, credential.userId);
      navigate('/', { replace: true });
    } catch (caught) {
      if (attemptRef.current !== attempt) return;
      setError(describeDesktopOAuthError(caught));
    } finally {
      if (attemptRef.current === attempt) setSigningIn(false);
    }
  };

  const cancelSignIn = () => {
    attemptRef.current += 1;
    setSigningIn(false);
    setError('Sign-in was cancelled. Try again when ready.');
  };

  return (
    <main className="login-page" data-tauri-drag-region="deep">
      <section className="login-column" aria-label="CoWiki sign in">
        <img className="cowiki-logo login-logo" src="/cowiki-logo.svg" alt="" width={40} height={40} />
        <h1>Connect what people and AI agents know.</h1>
        <p className="login-tagline">CoWiki · a collaborative LLM wiki</p>

        <div className="login-actions">
          <a
            className="login-github-button"
            href={buildWebGithubLoginUrl(apiBase())}
            aria-busy={signingIn || undefined}
            aria-disabled={signingIn || undefined}
            data-pending={signingIn ? 'true' : undefined}
            onClick={signInWithGitHub}
          >
            {signingIn ? (
              <LoaderCircle size={16} className="animate-spin" aria-hidden="true" />
            ) : (
              <svg aria-hidden="true" width="17" height="17" viewBox="0 0 24 24" fill="currentColor">
                <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
              </svg>
            )}
            {signingIn ? 'Waiting for GitHub…' : desktop ? 'Continue with GitHub' : 'Sign in with GitHub'}
          </a>

          {desktop && (
            <button
              className="login-local-button"
              type="button"
              onClick={() => navigate('/', { replace: true })}
            >
              Continue locally
            </button>
          )}
        </div>

        {signingIn && (
          <p className="login-status" role="status">
            Finish signing in in your browser.{' '}
            <button type="button" className="login-cancel-button" onClick={cancelSignIn}>
              Cancel
            </button>
          </p>
        )}

        {error && (
          <p className="login-error" role="alert">
            {error}
          </p>
        )}
      </section>
    </main>
  );
}
