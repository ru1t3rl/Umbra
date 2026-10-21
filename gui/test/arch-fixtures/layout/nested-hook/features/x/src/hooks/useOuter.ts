export function useOuter() {
  const useInner = () => 1;
  return useInner();
}
