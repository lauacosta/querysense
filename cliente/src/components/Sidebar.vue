<script setup lang="ts">
import type { HistorialRow } from '@/types/HistorialRow';
import { ref } from 'vue';

const rows = ref<HistorialRow[]>([]);
const query = ref('');

const handleQuery = (event: Event) => {
    const target = event.target as HTMLInputElement;
    query.value = target.value;
};

const handleForm = async (event: Event) => {
    event.preventDefault();
    try {
        const response = await fetch(`/search?query=${query.value}&doc=historial`);
        if (response.status !== 200) {
            throw new Error(`HTTP ERROR! status: ${response.status}`);
        }
        const data: HistorialRow[] = await response.json();
        rows.value = data;
    } catch (error) {
        console.error("Hubo un fallo al pedir la información", error);
    }
};

defineProps<{
    props: HistorialRow[],
}>();

</script>
<template>
    <div id="sidebar">
        <div>
            <form class="grid" id="form-input" @submit.prevent="handleForm">
                <input type="text" id="search-input" aria-placeholder="Escribe tu consulta..." v-model="query"
                    placeholder="Ingrese su búsqueda" @input="handleQuery" />
            </form>
        </div>

        <ul id="historial">
            <li v-for="row in props" :key="row.id">
                {{ row.query }}
            </li>
        </ul>
    </div>
</template>

<style>
#sidebar {
    padding: auto;
    width: 18rem;
    height: 67.7rem;
    background-color: #e0e0e0;
    overflow-y: auto;
    border: 1px solid rgb(36 36 36);
    overflow-x: hidden;
}

#historial {
    list-style-type: none;
    padding: 0;
    margin: 0;
    border-bottom: 1px solid #ccc;
    color: #242424
}

#historial li {
    padding: 10px;
    border-bottom: 1px solid #ccc;
    color: #242424;
    cursor: pointer;
}

#historial li:hover {
    background-color: #eaeaea;
}
</style>
