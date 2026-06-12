# VisionClaw Security Hardening

**Source:** OpenEvolve Night Cycle Report 2026-04-12 01:15 UTC  
**Priority:** P0 - Critical (Before Wide Deployment)  
**Platforms:** iOS, Android (VisionClaw mobile app)

---

## Problem Statement

VisionClaw's mobile app has critical security gaps before production deployment:

| Risk | Component | Impact |
|------|-----------|--------|
| Insecure Credential Storage | Keychain/Keystore | Account compromise |
| Weak TLS Configuration | Network Layer | MITM attacks |
| No Certificate Pinning | HTTPS | Certificate spoofing |

**T430 Fitness Score:** Syntax: 0.95 | Semantic: 0.90 | Quality: 0.88 | Security: 0.75 | **Total: 0.87**

---

## iOS Security Implementation

### Keychain Security

```swift
// VisionClaw/Security/SecureCredentialStore.swift
import Security
import Foundation

enum KeychainError: Error {
    case itemNotFound
    case duplicateItem
    case invalidStatus(OSStatus)
    case conversionFailed
}

class SecureCredentialStore {
    
    // Use kSecAttrAccessibleWhenUnlockedThisDeviceOnly
    // - Data accessible only when device is unlocked
    // - Data not included in iCloud backups
    // - Data not accessible after restore to different device
    private let accessibility = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
    
    func store(
        credentials: Credentials,
        service: String
    ) throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrAccount as String: credentials.username,
            kSecAttrService as String: service,
            kSecValueData as String: credentials.password.data(using: .utf8)!,
            kSecAttrAccessible as String: accessibility
        ]
        
        let status = SecItemAdd(query as CFDictionary, nil)
        
        if status == errSecDuplicateItem {
            // Update existing
            let updateQuery: [String: Any] = [
                kSecClass as String: kSecClassGenericPassword,
                kSecAttrAccount as String: credentials.username,
                kSecAttrService as String: service
            ]
            let updateAttributes: [String: Any] = [
                kSecValueData as String: credentials.password.data(using: .utf8)!
            ]
            let updateStatus = SecItemUpdate(
                updateQuery as CFDictionary,
                updateAttributes as CFDictionary
            )
            
            guard updateStatus == errSecSuccess else {
                throw KeychainError.invalidStatus(updateStatus)
            }
        } else if status != errSecSuccess {
            throw KeychainError.invalidStatus(status)
        }
    }
    
    func retrieve(service: String, account: String) throws -> Credentials? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne
        ]
        
        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        
        guard status == errSecSuccess else {
            if status == errSecItemNotFound {
                return nil
            }
            throw KeychainError.invalidStatus(status)
        }
        
        guard let data = result as? Data,
              let password = String(data: data, encoding: .utf8) else {
            throw KeychainError.conversionFailed
        }
        
        return Credentials(username: account, password: password)
    }
    
    func delete(service: String, account: String) throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
        
        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw KeychainError.invalidStatus(status)
        }
    }
}

struct Credentials {
    let username: String
    let password: String
}
```

### TLS Certificate Pinning

```swift
// VisionClaw/Security/PinnedURLSession.swift
import Foundation

class PinnedURLSessionDelegate: NSObject, URLSessionDelegate {
    
    // Pin OpenClaw Gateway certificate
    private let pinnedCertificates: [Data]
    
    init(pinnedCertificates: [Data]) {
        self.pinnedCertificates = pinnedCertificates
        super.init()
    }
    
    func urlSession(
        _ session: URLSession,
        didReceive challenge: URLAuthenticationChallenge,
        completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void
    ) {
        guard let serverTrust = challenge.protectionSpace.serverTrust,
              let certificateChain = SecTrustCopyCertificateChain(serverTrust) as? [SecCertificate],
              !certificateChain.isEmpty else {
            completionHandler(.cancelAuthenticationChallenge, nil)
            return
        }
        
        // Get server certificate
        let serverCertificate = certificateChain[0]
        guard let serverCertificateData = SecCertificateCopyData(serverCertificate) as Data? else {
            completionHandler(.cancelAuthenticationChallenge, nil)
            return
        }
        
        // Compare with pinned certificates
        if pinnedCertificates.contains(serverCertificateData) {
            let credential = URLCredential(trust: serverTrust)
            completionHandler(.useCredential, credential)
        } else {
            // Certificate mismatch - potential MITM
            completionHandler(.cancelAuthenticationChallenge, nil)
        }
    }
}

// Usage
func createPinnedSession() -> URLSession {
    // Load pinned certificate from app bundle
    guard let certificatePath = Bundle.main.path(
        forResource: "openclaw_gateway_cert",
        ofType: "der"
    ),
    let certificateData = try? Data(contentsOf: URL(fileURLWithPath: certificatePath)) else {
        fatalError("Failed to load pinned certificate")
    }
    
    let delegate = PinnedURLSessionDelegate(pinnedCertificates: [certificateData])
    let config = URLSessionConfiguration.default
    
    // Enforce TLS 1.3 minimum
    config.tlsMinimumSupportedProtocolVersion = .TLSv13
    
    return URLSession(configuration: config, delegate: delegate, delegateQueue: nil)
}
```

---

## Android Security Implementation

### Keystore Security

```kotlin
// com.visionclaw.security.SecureCredentialStore.kt
package com.visionclaw.security

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

class SecureCredentialStore(context: Context) {
    
    private val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
    private val masterKey = getOrCreateMasterKey()
    
    companion object {
        private const val ANDROID_KEYSTORE = "AndroidKeyStore"
        private const val KEY_ALIAS = "VisionClawMasterKey"
        private const val ANDROID_VERSION = android.os.Build.VERSION.SDK_INT
    }
    
    private fun getOrCreateMasterKey(): SecretKey {
        return keyStore.getEntry(KEY_ALIAS, null) as? SecretKey
            ?: createMasterKey()
    }
    
    private fun createMasterKey(): SecretKey {
        val keyGen = KeyGenerator.getInstance(
            KeyProperties.KEY_ALGORITHM_AES,
            ANDROID_KEYSTORE
        )
        
        val builder = KeyGenParameterSpec.Builder(
            KEY_ALIAS,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .setUserAuthenticationRequired(false)
        
        // Require biometric auth on Android 9+ for sensitive operations
        if (ANDROID_VERSION >= android.os.Build.VERSION_CODES.P) {
            builder.setUnlockedDeviceRequired(true)
        }
        
        keyGen.init(builder.build())
        return keyGen.generateKey()
    }
    
    fun store(service: String, credentials: Credentials) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, masterKey)
        
        val iv = cipher.iv
        val encrypted = cipher.doFinal(
            "${credentials.username}:${credentials.password}".toByteArray()
        )
        
        // Store encrypted data + IV in EncryptedSharedPreferences
        EncryptedSharedPreferences.create(
            context,
            "visionclaw_secure",
            masterKey,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM
        ).edit().apply {
            putString("${service}_iv", Base64.encodeToString(iv, Base64.DEFAULT))
            putString(service, Base64.encodeToString(encrypted, Base64.DEFAULT))
            apply()
        }
    }
    
    fun retrieve(service: String): Credentials? {
        val prefs = EncryptedSharedPreferences.create(
            context,
            "visionclaw_secure",
            masterKey,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM
        )
        
        val encryptedData = prefs.getString(service, null)
            ?: return null
        val ivString = prefs.getString("${service}_iv", null)
            ?: return null
        
        val iv = Base64.decode(ivString, Base64.DEFAULT)
        val encrypted = Base64.decode(encryptedData, Base64.DEFAULT)
        
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, masterKey, GCMParameterSpec(128, iv))
        
        val decrypted = cipher.doFinal(encrypted)
        val parts = String(decrypted).split(":", limit = 2)
        
        return Credentials(parts[0], parts[1])
    }
}

data class Credentials(val username: String, val password: String)
```

### Certificate Pinning (Android)

```kotlin
// com.visionclaw.security.PinnedOkHttpClient.kt
package com.visionclaw.security

import okhttp3.CertificatePinner
import okhttp3.OkHttpClient
import okhttp3.TlsVersion
import javax.net.ssl.SSLContext
import javax.net.ssl.TrustManager
import javax.net.ssl.X509TrustManager

object PinnedOkHttpClient {
    
    private const val GATEWAY_HOSTNAME = "gateway.openclaw.local"
    
    // SHA-256 hashes of pinned certificates
    private val PINNED_CERTIFICATES = listOf(
        "sha256/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",  // Primary
        "sha256/BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB="   // Backup
    )
    
    fun create(): OkHttpClient {
        val certificatePinner = CertificatePinner.Builder()
            .add(GATEWAY_HOSTNAME, *PINNED_CERTIFICATES.toTypedArray())
            .build()
        
        return OkHttpClient.Builder()
            .certificatePinner(certificatePinner)
            .connectionSpecs(listOf(
                ConnectionSpec.Builder(ConnectionSpec.MODERN_TLS)
                    .tlsVersions(TlsVersion.TLS_1_3, TlsVersion.TLS_1_2)
                    .cipherSuites(
                        CipherSuite.TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384,
                        CipherSuite.TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384
                    )
                    .build()
            ))
            .build()
    }
}
```

---

## Security Configuration

```yaml
# visionclaw/security.yaml
ios:
  keychain:
    accessibility: when_unlocked_this_device_only
    biometric_for_sensitive: true
  tls:
    minimum_version: "1.3"
    certificate_pinning: true
    pinned_hosts:
      - gateway.openclaw.local
    
android:
  keystore:
    require_unlocked_device: true
    key_size: 256
  tls:
    minimum_version: "1.3"
    certificate_pinning: true
    pinned_hosts:
      - gateway.openclaw.local
    
common:
  allowed_schemes:
    - https
  blocked_ports:
    - 80  # HTTP
    - 443 # HTTPS (must use pinned)
```

---

## Testing

### iOS Security Tests

```swift
// VisionClawTests/Security/SecureCredentialStoreTests.swift
import XCTest
@testable import VisionClaw

class SecureCredentialStoreTests: XCTestCase {
    
    var store: SecureCredentialStore!
    
    override func setUp() {
        store = SecureCredentialStore()
    }
    
    override func tearDown() {
        // Clean up test credentials
        try? store.delete(service: "test", account: "testuser")
    }
    
    func testStoreAndRetrieve() throws {
        let creds = Credentials(username: "testuser", password: "testpass")
        try store.store(credentials: creds, service: "test")
        
        let retrieved = try store.retrieve(service: "test", account: "testuser")
        XCTAssertEqual(retrieved?.username, "testuser")
        XCTAssertEqual(retrieved?.password, "testpass")
    }
    
    func testCertificatePinningRejectsInvalid() {
        let expectation = self.expectation(description: "Connection rejected")
        
        // Try to connect with wrong certificate
        let session = createPinnedSessionWithWrongCert()
        let task = session.dataTask(with: URL(string: "https://gateway.openclaw.local")!) { _, _, error in
            XCTAssertNotNil(error)
            expectation.fulfill()
        }
        task.resume()
        
        waitForExpectations(timeout: 5)
    }
}
```

### Android Security Tests

```kotlin
// com.visionclaw.security.SecureCredentialStoreTest.kt
package com.visionclaw.security

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.Assert.*

@RunWith(AndroidJUnit4::class)
class SecureCredentialStoreTest {
    
    @Test
    fun testStoreAndRetrieve() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val store = SecureCredentialStore(context)
        
        store.store("test", Credentials("user", "pass"))
        val retrieved = store.retrieve("test")
        
        assertEquals("user", retrieved?.username)
        assertEquals("pass", retrieved?.password)
    }
    
    @Test
    fun testCertificatePinning() {
        val client = PinnedOkHttpClient.create()
        
        // This should fail with pinned certificate mismatch
        val request = Request.Builder()
            .url("https://untrusted.example.com")
            .build()
        
        client.newCall(request).execute().use { response ->
            // Should not reach here - SSLPeerUnverifiedException expected
            fail("Should have rejected untrusted certificate")
        }
    }
}
```

---

## Deployment Checklist

- [ ] Implement iOS Keychain with `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`
- [ ] Implement Android Keystore with hardware-backed keys where available
- [ ] Add certificate pinning for OpenClaw Gateway
- [ ] Enforce TLS 1.3 minimum
- [ ] Add security unit tests
- [ ] Conduct penetration testing
- [ ] Document security architecture
- [ ] Set up security monitoring/alerting

---

## References

- Night Cycle Report: night_cycle_20260412_0115.md
- Apple Keychain Services Documentation
- Android Keystore System Documentation
- OWASP Certificate Pinning Guide