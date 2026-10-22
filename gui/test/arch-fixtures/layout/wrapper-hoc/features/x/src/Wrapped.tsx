declare function withAuth<T>(c: T): T;
declare const Inner: () => null;
export const Wrapped = withAuth(Inner);
