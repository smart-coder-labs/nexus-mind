export const products = [
  { id: 'p1', name: 'Auriculares Pro', category: 'Audio', price: 99 },
  { id: 'p2', name: 'Teclado Mecánico', category: 'Oficina', price: 149 },
  { id: 'p3', name: 'Altavoz Mini', category: 'Audio', price: 39 },
  { id: 'p4', name: 'Soporte Laptop', category: 'Oficina', price: 29 },
];

export function selectProducts(items, { query = '', category = 'all', sort = 'name' } = {}) {
  return items;
}

export function renderProducts(items) {
  return items.map((item) => `<p>${item.name}</p>`).join('');
}

if (typeof document !== 'undefined') {
  document.querySelector('#app').innerHTML = renderProducts(products);
}
