@preconcurrency import Citadel
import Crypto
import Foundation
@preconcurrency import NIOSSH

/// Authentication-method construction + error-classification helpers,
/// split out of `CitadelSSHClient.swift` so the controller stays under
/// SwiftLint's `file_length` cap.
extension CitadelSSHClient {
    static func makeAuthMethod(
        for credential: HostCredential
    ) throws -> SSHAuthenticationMethod {
        switch credential.auth {
        case let .password(password):
            return SSHAuthenticationMethod.passwordBased(
                username: credential.username,
                password: password
            )

        case let .privateKey(keyData, passphrase):
            let passphraseData = passphrase.flatMap { Data($0.utf8) }

            // Try ed25519 first, then RSA. Citadel 0.12.1 ships only these two
            // OpenSSH private-key parsers; other curves throw `.privateKeyParse`.
            if let ed = try? Curve25519.Signing.PrivateKey(
                sshEd25519: keyData,
                decryptionKey: passphraseData
            ) {
                return SSHAuthenticationMethod.ed25519(
                    username: credential.username,
                    privateKey: ed
                )
            }

            do {
                let rsa = try Insecure.RSA.PrivateKey(
                    sshRsa: keyData,
                    decryptionKey: passphraseData
                )
                return SSHAuthenticationMethod.rsa(
                    username: credential.username,
                    privateKey: rsa
                )
            } catch {
                // Distinguish "needs passphrase" from "garbage / wrong
                // passphrase": if caller supplied no passphrase but the
                // failure mentions the KDF/cipher path, ask for one.
                // Otherwise report a parse error.
                if passphraseData == nil, Self.errorMentionsEncryption(error) {
                    throw SSHError.privateKeyPassphraseRequired
                }
                throw SSHError.privateKeyParse
            }
        }
    }

    private static func errorMentionsEncryption(_ error: Error) -> Bool {
        let text = String(describing: error).lowercased()
        return text.contains("bcrypt") || text.contains("cipher")
            || text.contains("decrypt") || text.contains("kdf")
            || text.contains("missingdecryptionkey")
    }

    static func classifyConnectError(_ error: Error) -> SSHError {
        if let ssh = error as? SSHError { return ssh }
        if error is NIOSSH.NIOSSHError {
            let text = String(describing: error).lowercased()
            if text.contains("auth") { return .authenticationFailed }
            return .handshakeFailed(String(describing: error))
        }
        if error is InvalidHostKey {
            // Should be caught earlier via TOFUHostKeyDelegate.Mismatch,
            // but be safe.
            return .hostKeyMismatch(stored: "", remote: "")
        }
        return SSHErrorMapping.map(error)
    }
}
