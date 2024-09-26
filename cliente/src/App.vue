<script setup lang="ts">
import { ref } from 'vue';
import type { TableRow } from './types/TableRow'
import type { HistorialRow } from './types/HistorialRow.ts'
import Tabla from './components/Tabla.vue'
import Sidebar from './components/Sidebar.vue'
import { onMounted } from 'vue';

const rows = ref<TableRow[]>([]);
const historial = ref<HistorialRow[]>([]);
const query = ref('');

const fetchHistorial = async () => {
    try {
        let response = await fetch(`http://localhost:3000/historial`);
        if (response.status !== 200 && response.status !== 204) {
            throw new Error(`HTTP ERROR! status: ${response.status} `);
        }
        let data: HistorialRow[] = await response.json();
        historial.value = [...data].sort((a, b) => b.id - a.id);
        console.table(historial.value)
    } catch (error) {
        console.error("Hubo un fallo al pedir la información", error);
    }
}

const handleForm = async () => {
    try {
        const response = await fetch(`http://localhost:3000/search?query=${query.value}&doc=tnea`);
        if (response.status !== 200) {
            throw new Error(`HTTP ERROR! status: ${response.status} `);
        }
        const data: TableRow[] = await response.json();
        rows.value = data;
        console.log(rows.value)
        await fetchHistorial()
    } catch (error) {
        console.error("Hubo un fallo al pedir la información", error);
    }
}

onMounted(() => {
    fetchHistorial()
});

</script>

<template>
    <h1>TNEA Gestión</h1>
    <hr>
    </hr>
    <div id="layout">
        <Sidebar :props="historial" />
        <div id="contenido">
            <form className="grid" id="form-input" @submit.prevent="handleForm">
                <input type="text" id="search-input" placeholder="Ingrese su búsqueda"
                    aria-placeholder="Escribe tu consulta..." v-model="query" />
                <button type="submit" id="search-button">Buscar</button>
            </form>
            <div>
                <p>Cantidad de resultados: {{ rows.length }}</p>
            </div>
            <div v-if="rows.length > 0">
                <Tabla :props="rows" />
            </div>
        </div>

    </div>

</template>

<style scoped>
#layout {
    display: flex;
    height: 100vh;
}

#contenido {
    flex: 1;
    padding: 20px;
    overflow-y: auto;
}
</style>
