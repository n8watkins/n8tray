// Checks every tracked text file: no em dash (the project writes a plain "-"),
// no carriage returns (the repo is LF only) and a final newline.
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'

const files = execFileSync('git', ['ls-files', '-z'], { encoding: 'utf8' }).split('\0').filter(Boolean)
const binary = /\.(png|ico|icns|jpg|jpeg|gif|webp|woff2?|ttf|zip|wav)$/i
const problems = []
for (const file of files) {
  if (binary.test(file)) continue
  let text
  try {
    text = readFileSync(file, 'utf8')
  } catch {
    continue // deleted in the working tree
  }
  const lines = text.split('\n')
  lines.forEach((line, i) => {
    if (line.includes('\u2014')) problems.push(`${file}:${i + 1}: em dash; use a plain "-"`)
    if (line.includes('\r')) problems.push(`${file}:${i + 1}: carriage return; the repo is LF only`)
  })
  if (text.length && !text.endsWith('\n')) problems.push(`${file}: no final newline`)
}
if (problems.length) {
  console.error(problems.join('\n'))
  console.error(`\n${problems.length} problem(s) in tracked text files.`)
  process.exit(1)
}
console.log(`Text check: ${files.length} tracked files are clean.`)
