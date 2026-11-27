/** Remove secrets from a log line. Empty secrets are ignored. */
export function redact(text: string, secrets: Array<string | undefined>): string {
  let output = text;
  for (const secret of secrets) {
    if (!secret || secret.length < 4) continue;
    output = output.split(secret).join("[redacted]");
  }
  return output;
}
