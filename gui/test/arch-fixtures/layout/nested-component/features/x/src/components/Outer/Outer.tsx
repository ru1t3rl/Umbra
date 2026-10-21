export function Outer() {
  function Inner() {
    return <i />;
  }
  return <Inner />;
}
