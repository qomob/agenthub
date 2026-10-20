let counter = 0
let locks = 0
let previousOverflow = ''
const stack: number[] = []

function lockBody(): void {
  if (typeof document === 'undefined') return
  if (locks === 0) {
    previousOverflow = document.body.style.overflow
    document.body.style.overflow = 'hidden'
  }
  locks += 1
}

function unlockBody(): void {
  if (typeof document === 'undefined') return
  locks = Math.max(0, locks - 1)
  if (locks === 0) {
    document.body.style.overflow = previousOverflow
  }
}

export function pushModal(): number {
  counter += 1
  stack.push(counter)
  lockBody()
  return counter
}

export function popModal(handle: number): void {
  const index = stack.indexOf(handle)
  if (index >= 0) stack.splice(index, 1)
  unlockBody()
}

export function isTopModal(handle: number): boolean {
  return stack[stack.length - 1] === handle
}
