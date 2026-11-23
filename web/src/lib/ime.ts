/**
 * An Enter that confirms an input-method candidate (Chinese, Japanese, Korean)
 * must not send or submit. Chromium marks it `isComposing`; WebKit, which the
 * macOS app runs on, sends it after the composition ends with key code 229.
 */
export function isComposingKey(event: { nativeEvent?: KeyboardEvent; isComposing?: boolean; keyCode?: number }): boolean {
  const native = event.nativeEvent ?? event;
  return !!native.isComposing || native.keyCode === 229;
}

/** Enter that should submit: not Shift+Enter, not an input-method confirmation. */
export function isSubmitEnter(event: { key: string; shiftKey?: boolean; nativeEvent?: KeyboardEvent; isComposing?: boolean; keyCode?: number }): boolean {
  return event.key === 'Enter' && !event.shiftKey && !isComposingKey(event);
}
