//! Browser fingerprint normalization to defeat naive anti-bot detection.
//!
//! Port of the main evasions from `puppeteer-extra-plugin-stealth`.
//! Each evasion is a small JS snippet that overrides a property,
//! injects a missing API, or normalizes a value that headless Chrome
//! exposes inconsistently with a real browser.
//!
//! Defeats: navigator.webdriver, missing chrome.runtime/app/csi/loadTimes,
//! permissions inconsistency, empty plugins, headless UA leak, WebGL
//! Mesa vendor, Notification.permission, outerWidth=0, iframe defaultView,
//! H.264 codec rejection, native code toString detection, userAgentData,
//! Battery API, navigator.languages, vendor, deviceMemory,
//! hardwareConcurrency.
//!
//! Limits: does NOT defeat JA3/JA4 TLS fingerprint (Chromium handles
//! that correctly natively), CDP `Runtime.enable` detection, or
//! behavioral biometrics (mouse curves, typing cadence).

pub const REALISTIC_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36";

pub const REALISTIC_PLATFORM: &str = "Linux";

#[must_use]
pub fn build_stealth_script() -> &'static str {
    STEALTH_JS
}

#[must_use]
pub const fn realistic_user_agent() -> &'static str {
    REALISTIC_USER_AGENT
}

const STEALTH_JS: &str = r"
(() => {
  'use strict';
  try { Object.defineProperty(Navigator.prototype, 'webdriver', { get: () => undefined, configurable: true }); } catch (_) {}

  if (!window.chrome) { window.chrome = {}; }
  if (!window.chrome.runtime) {
    Object.defineProperty(window.chrome, 'runtime', {
      value: {
        OnInstalledReason: { CHROME_UPDATE: 'chrome_update', INSTALL: 'install', SHARED_MODULE_UPDATE: 'shared_module_update', UPDATE: 'update' },
        OnRestartRequiredReason: { APP_UPDATE: 'app_update', OS_UPDATE: 'os_update', PERIODIC: 'periodic' },
        PlatformArch: { ARM: 'arm', ARM64: 'arm64', MIPS: 'mips', MIPS64: 'mips64', X86_32: 'x86-32', X86_64: 'x86-64' },
        PlatformOs: { ANDROID: 'android', CROS: 'cros', LINUX: 'linux', MAC: 'mac', OPENBSD: 'openbsd', WIN: 'win' },
        RequestUpdateCheckStatus: { NO_UPDATE: 'no_update', THROTTLED: 'throttled', UPDATE_AVAILABLE: 'update_available' },
        connect: function () {}, sendMessage: function () {},
      },
      writable: false, enumerable: true, configurable: false,
    });
  }
  if (!window.chrome.app) {
    Object.defineProperty(window.chrome, 'app', {
      value: {
        isInstalled: false,
        InstallState: { DISABLED: 'disabled', INSTALLED: 'installed', NOT_INSTALLED: 'not_installed' },
        RunningState: { CANNOT_RUN: 'cannot_run', READY_TO_RUN: 'ready_to_run', RUNNING: 'running' },
        getDetails: function () { return null; }, getIsInstalled: function () { return false; },
        installState: function () { return 'not_installed'; }, runningState: function () { return 'cannot_run'; },
      },
      writable: false, enumerable: true, configurable: false,
    });
  }
  if (!window.chrome.csi) {
    window.chrome.csi = function () {
      return { startE: Date.now() - Math.floor(Math.random() * 1000), onloadT: Date.now(), pageT: Math.floor(Math.random() * 5000), tran: 15 };
    };
  }
  if (!window.chrome.loadTimes) {
    window.chrome.loadTimes = function () {
      const t = Date.now() / 1000;
      return {
        requestTime: t - 1.2, startLoadTime: t - 1.1, commitLoadTime: t - 1.0,
        finishDocumentLoadTime: t - 0.5, finishLoadTime: t - 0.1,
        firstPaintTime: t - 0.4, firstPaintAfterLoadTime: 0,
        navigationType: 'Other', wasFetchedViaSpdy: true, wasNpnNegotiated: true,
        npnNegotiatedProtocol: 'h2', wasAlternateProtocolAvailable: false, connectionInfo: 'h2',
      };
    };
  }

  try {
    const origQuery = window.navigator.permissions.query.bind(window.navigator.permissions);
    window.navigator.permissions.query = (p) => {
      if (p && p.name === 'notifications') {
        return Promise.resolve({ state: Notification.permission, onchange: null });
      }
      return origQuery(p);
    };
  } catch (_) {}

  try {
    const mimeTypes = [
      { type: 'application/pdf', suffixes: 'pdf', description: 'Portable Document Format' },
      { type: 'application/x-google-chrome-pdf', suffixes: 'pdf', description: 'Portable Document Format' },
      { type: 'application/x-nacl', suffixes: '', description: 'Native Client Executable' },
      { type: 'application/x-pnacl', suffixes: '', description: 'Portable Native Client Executable' },
    ];
    const plugins = [
      { name: 'Chrome PDF Plugin', filename: 'internal-pdf-viewer', description: 'Portable Document Format', length: 1 },
      { name: 'Chrome PDF Viewer', filename: 'mhjfbmdgcfjbbpaeojofohoefgiehjai', description: '', length: 1 },
      { name: 'Native Client', filename: 'internal-nacl-plugin', description: '', length: 2 },
    ];
    Object.defineProperty(navigator, 'plugins', {
      get: () => {
        const arr = plugins.map(p => Object.assign(Object.create(Plugin.prototype), p));
        arr.length = plugins.length;
        arr.namedItem = function (n) { return arr.find(p => p.name === n) || null; };
        arr.refresh = function () {};
        arr.item = function (i) { return arr[i] || null; };
        return arr;
      },
      configurable: true,
    });
    Object.defineProperty(navigator, 'mimeTypes', {
      get: () => {
        const arr = mimeTypes.map(m => Object.assign(Object.create(MimeType.prototype), m));
        arr.length = mimeTypes.length;
        arr.namedItem = function (n) { return arr.find(m => m.type === n) || null; };
        arr.item = function (i) { return arr[i] || null; };
        return arr;
      },
      configurable: true,
    });
  } catch (_) {}

  try { Object.defineProperty(navigator, 'languages', { get: () => ['en-US', 'en'], configurable: true }); } catch (_) {}
  try { Object.defineProperty(navigator, 'vendor', { get: () => 'Google Inc.', configurable: true }); } catch (_) {}
  try { if (navigator.hardwareConcurrency < 4) { Object.defineProperty(navigator, 'hardwareConcurrency', { get: () => 8, configurable: true }); } } catch (_) {}
  try { Object.defineProperty(navigator, 'deviceMemory', { get: () => 8, configurable: true }); } catch (_) {}

  try {
    const getParameter = WebGLRenderingContext.prototype.getParameter;
    WebGLRenderingContext.prototype.getParameter = function (p) {
      if (p === 37445) return 'Intel Inc.';
      if (p === 37446) return 'Intel Iris OpenGL Engine';
      return getParameter.call(this, p);
    };
    if (typeof WebGL2RenderingContext !== 'undefined') {
      const getParameter2 = WebGL2RenderingContext.prototype.getParameter;
      WebGL2RenderingContext.prototype.getParameter = function (p) {
        if (p === 37445) return 'Intel Inc.';
        if (p === 37446) return 'Intel Iris OpenGL Engine';
        return getParameter2.call(this, p);
      };
    }
  } catch (_) {}

  try {
    if (window.Notification && Notification.permission === 'denied') {
      Object.defineProperty(Notification, 'permission', { get: () => 'default', configurable: true });
    }
  } catch (_) {}

  try {
    if (window.outerWidth === 0) Object.defineProperty(window, 'outerWidth', { get: () => window.innerWidth, configurable: true });
    if (window.outerHeight === 0) Object.defineProperty(window, 'outerHeight', { get: () => window.innerHeight + 85, configurable: true });
  } catch (_) {}

  try {
    const origDesc = Object.getOwnPropertyDescriptor(HTMLIFrameElement.prototype, 'contentWindow');
    if (origDesc && origDesc.get) {
      Object.defineProperty(HTMLIFrameElement.prototype, 'contentWindow', {
        get: function () {
          const orig = origDesc.get.call(this);
          try {
            const doc = this.contentDocument;
            if (doc && doc.defaultView && doc.defaultView !== orig) return doc.defaultView;
          } catch (_) {}
          return orig;
        },
        configurable: true,
      });
    }
  } catch (_) {}

  try {
    const PROBABLY = ['video/mp4', 'video/webm', 'audio/mpeg', 'audio/mp4', 'audio/webm', 'video/ogg'];
    const orig = HTMLMediaElement.prototype.canPlayType;
    HTMLMediaElement.prototype.canPlayType = function (type) {
      if (typeof type === 'string') {
        const base = type.split(';')[0].trim();
        if (PROBABLY.includes(base)) return 'probably';
      }
      return orig.call(this, type);
    };
    if (typeof MediaSource !== 'undefined' && MediaSource.isTypeSupported) {
      const origMS = MediaSource.isTypeSupported;
      MediaSource.isTypeSupported = function (type) {
        if (typeof type === 'string') {
          const base = type.split(';')[0].trim();
          if (PROBABLY.includes(base)) return true;
        }
        return origMS.call(this, type);
      };
    }
  } catch (_) {}

  const patchedFunctions = new WeakSet();
  const origToString = Function.prototype.toString;
  Function.prototype.toString = function () {
    if (patchedFunctions.has(this)) {
      return 'function ' + (this.name || '') + '() { [native code] }';
    }
    return origToString.call(this);
  };
  [HTMLMediaElement.prototype.canPlayType, WebGLRenderingContext.prototype.getParameter].forEach(fn => {
    if (typeof fn === 'function') { try { patchedFunctions.add(fn); } catch (_) {} }
  });

  try {
    if (!navigator.userAgentData) {
      Object.defineProperty(navigator, 'userAgentData', {
        get: () => ({
          brands: [
            { brand: 'Chromium', version: '123' },
            { brand: 'Google Chrome', version: '123' },
            { brand: 'Not.A/Brand', version: '8' },
          ],
          mobile: false,
          platform: 'Linux',
          getHighEntropyValues: function (hints) {
            return Promise.resolve({
              architecture: 'x86', bitness: '64', brands: this.brands,
              fullVersionList: [
                { brand: 'Chromium', version: '123.0.0.0' },
                { brand: 'Google Chrome', version: '123.0.0.0' },
                { brand: 'Not.A/Brand', version: '8.0.0.0' },
              ],
              mobile: false, model: '', platform: 'Linux',
              platformVersion: '6.1.0', uaFullVersion: '123.0.0.0', wow64: false,
            });
          },
          toJSON: function () { return { brands: this.brands, mobile: this.mobile, platform: this.platform }; },
        }),
        configurable: true,
      });
    }
  } catch (_) {}

  try {
    if (navigator.getBattery) {
      const orig = navigator.getBattery.bind(navigator);
      navigator.getBattery = () => orig().then(b => b).catch(() => ({
        charging: true, chargingTime: 0, dischargingTime: Infinity, level: 0.92,
        addEventListener: () => {}, removeEventListener: () => {},
      }));
    } else {
      navigator.getBattery = () => Promise.resolve({
        charging: true, chargingTime: 0, dischargingTime: Infinity, level: 0.92,
        addEventListener: () => {}, removeEventListener: () => {},
      });
    }
  } catch (_) {}

  try {
    if (navigator.connection) {
      Object.defineProperty(navigator.connection, 'rtt', { get: () => 50, configurable: true });
      Object.defineProperty(navigator.connection, 'downlink', { get: () => 10, configurable: true });
      Object.defineProperty(navigator.connection, 'effectiveType', { get: () => '4g', configurable: true });
      Object.defineProperty(navigator.connection, 'saveData', { get: () => false, configurable: true });
    }
  } catch (_) {}
})();
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stealth_script_balanced_braces() {
        let s = build_stealth_script();
        assert_eq!(s.matches('{').count(), s.matches('}').count());
        assert_eq!(s.matches('(').count(), s.matches(')').count());
    }

    #[test]
    fn stealth_covers_core_evasions() {
        let s = build_stealth_script();
        for keyword in [
            "webdriver",
            "chrome.runtime",
            "chrome.app",
            "chrome.loadTimes",
            "permissions",
            "plugins",
            "languages",
            "WebGL",
            "Notification",
            "outerWidth",
            "HTMLIFrameElement",
            "userAgentData",
            "getBattery",
        ] {
            assert!(s.contains(keyword), "stealth missing evasion: {keyword}");
        }
    }

    #[test]
    fn ua_no_headless_token() {
        assert!(!realistic_user_agent().to_lowercase().contains("headless"));
        assert!(realistic_user_agent().contains("Chrome/"));
    }
}
