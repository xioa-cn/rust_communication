self.onmessage = (event: MessageEvent<{ text: string; query: string; regex: boolean }>) => {
  try {
    const { text, query, regex } = event.data
    if (!query) { self.postMessage({ indices: [] }); return }
    const indices: number[] = []
    if (regex) {
      for (const match of text.matchAll(new RegExp(query, 'g'))) {
        indices.push(match.index)
        if (indices.length >= 1000) break
      }
    } else {
      let index = text.indexOf(query)
      while (index >= 0 && indices.length < 1000) { indices.push(index); index = text.indexOf(query, index + Math.max(query.length, 1)) }
    }
    self.postMessage({ indices })
  } catch (error) { self.postMessage({ error: String(error) }) }
}
