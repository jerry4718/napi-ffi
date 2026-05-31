'use strict'
async function gcUntil(_name, condition, tries = 20) {
  for (let i = 0; i < tries; i++) {
    if (global.gc) global.gc()
    if (condition()) return
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}
module.exports = { gcUntil }
