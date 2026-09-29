/** The last part of a slash path: the name that a row or a chip shows. */
export const basename = (p: string) => p.split('/').pop() ?? p;
