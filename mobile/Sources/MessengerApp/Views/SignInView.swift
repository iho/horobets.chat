import SwiftUI

struct SignInView: View {
    @ObservedObject var core: CoreAdapter
    @State private var server = "https://"
    @State private var email = ""
    @State private var password = ""
    @State private var register = false
    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text("h.").font(.largeTitle.bold()).foregroundStyle(.teal)
                    Text(register ? "Make room for your team." : "Welcome back.").font(.title2.bold())
                    Text("Your conversations. Your server.").foregroundStyle(.secondary)
                }
                Section("Your server") {
                    TextField("https://chat.example.com", text: $server).autocorrectionDisabled()
                    Text("Enter the server address provided by your workspace administrator.").font(.caption)
                }
                Section("Account") {
                    TextField("Email", text: $email).autocorrectionDisabled()
                    SecureField("Password", text: $password)
                }
                Section {
                    Button(core.snapshot.busy ? "Connecting…" : (register ? "Create account" : "Sign in")) {
                        core.command("authenticate", payload: ["server": server.trimmingCharacters(in: .whitespacesAndNewlines), "email": email.trimmingCharacters(in: .whitespacesAndNewlines), "password": password, "register": register])
                        password = ""
                    }.disabled(core.snapshot.busy || email.isEmpty || password.isEmpty)
                    Button(register ? "Already have an account?" : "Create an account") { register.toggle() }
                }
            }.navigationTitle("horobets.chat")
        }
    }
}
