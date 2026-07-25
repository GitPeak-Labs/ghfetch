export function createFakeExecutionContext(): ExecutionContext {
  return {
    waitUntil: (promise: Promise<unknown>) => {
      promise.catch((error) => console.error("Unhandled waitUntil rejection in test", error))
    },
    passThroughOnException: () => {},
  } as unknown as ExecutionContext
}
